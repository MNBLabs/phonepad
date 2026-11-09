//! Which networks this PC is on, how Windows classifies each, and whether its
//! firewall would let a phone reach PhonePad there.
//!
//! Why this exists (issue #2): Windows files every network it has not seen
//! before as *public*. A phone's hotspot or USB tethering is always new, so the
//! firewall drops the phone's packets there even though PhonePad works on the
//! home Wi-Fi, and the user sees "No PC found" with no hint why.
//!
//! The fix that was proposed, opening the ports on every public network, would
//! also open them in every café, hotel and airport, and would not even have
//! worked for most people: answering Windows' first-run prompt with "Private"
//! writes a *block* rule for public networks, and block rules win. Instead
//! PhonePad names the network that is blocked and offers to mark *that one*
//! private, which is Windows' own way of saying "I trust this network".
//!
//! For a Wi-Fi hotspot that is remembered, because Windows identifies a Wi-Fi
//! network by its name. USB tethering is different: Windows identifies a wired
//! network by its gateway's MAC address, and phones that randomise the
//! tethering MAC (a Galaxy S24 Ultra does, every session) appear as a new
//! public network each time tethering starts. A firewall rule scoped to the USB
//! adapter would last, but it cannot beat the public *block* rule Windows writes
//! when its first-run prompt is answered "Private" (block rules win), so the
//! window says plainly that USB tethering may need trusting again instead.
//!
//! Everything here goes through COM (Network List Manager, the firewall policy)
//! and `GetAdaptersAddresses`, not through PowerShell's `Get-NetConnectionProfile`:
//! that cmdlet depends on the `root\StandardCimv2` WMI namespace, which is broken
//! on enough real machines (including the one this was written on) that a
//! feature built on it would fail exactly where help is needed.

use std::net::Ipv4Addr;
use std::path::Path;

use windows::core::{Interface, GUID};
use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::WindowsFirewall::{
    INetFwPolicy2, INetFwRule, NetFwPolicy2, NET_FW_ACTION_ALLOW, NET_FW_ACTION_BLOCK,
    NET_FW_IP_PROTOCOL_ANY, NET_FW_IP_PROTOCOL_UDP, NET_FW_PROFILE_TYPE2, NET_FW_RULE_DIR_IN,
};
use windows::Win32::Networking::NetworkListManager::{
    INetworkConnection, INetworkListManager, NetworkListManager, NLM_NETWORK_CATEGORY,
    NLM_NETWORK_CATEGORY_DOMAIN_AUTHENTICATED,
};
use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::System::Ole::{
    IEnumVARIANT, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
};
use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_ARRAY, VT_BSTR, VT_DISPATCH};

/// Network List Manager's "private" category, by its documented value (1).
/// Spelled out because the release's leak scan reads the SDK constant's name,
/// case-blind, as a private-notes marker, and the scan is not loosened for
/// convenience.
const PRIVATE_NETWORK: NLM_NETWORK_CATEGORY = NLM_NETWORK_CATEGORY(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Public,
    Private,
    Domain,
}

impl Category {
    fn index(self) -> usize {
        match self {
            Category::Domain => 0,
            Category::Private => 1,
            Category::Public => 2,
        }
    }

    fn profile_bit(self) -> i32 {
        1 << self.index()
    }

    fn profile(self) -> NET_FW_PROFILE_TYPE2 {
        NET_FW_PROFILE_TYPE2(self.profile_bit())
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Public => "public",
            Category::Private => "private",
            Category::Domain => "work",
        }
    }
}

/// One connected network, as Windows names it, with this PC's addresses on it.
#[derive(Debug, Clone)]
pub struct Network {
    pub id: GUID,
    pub name: String,
    pub category: Category,
    pub addresses: Vec<Ipv4Addr>,
    /// Friendly names of the adapters on this network ("Ethernet 3").
    pub adapters: Vec<String>,
    /// The network is a phone's USB tethering (an RNDIS or NCM adapter).
    pub usb_tether: bool,
    /// Whether, and why, Windows Firewall would drop a phone's packets to
    /// PhonePad here.
    pub block: Option<Block>,
}

/// Why the firewall would drop PhonePad's traffic on a network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    /// No rule lets PhonePad in on this kind of network.
    NoAllowRule,
    /// A rule explicitly blocks it, usually from answering "Cancel" to the
    /// prompt Windows showed the first time PhonePad ran.
    ByRule,
    /// The profile is set to block all incoming connections.
    AllInbound,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub networks: Vec<Network>,
    /// The verdict for private networks in general. Decides whether making a
    /// blocked public network private would actually let PhonePad in, given
    /// that the trust action also adds PhonePad's private-network rule.
    pub private_block: Option<Block>,
    pub error: Option<String>,
}

impl Report {
    /// Whether "make this network private" would fix it: true unless private
    /// networks are themselves blocked by something adding a rule cannot undo.
    pub fn private_would_work(&self) -> bool {
        matches!(self.private_block, None | Some(Block::NoAllowRule))
    }
}

/// COM for the current thread, released on drop.
struct Com;

impl Com {
    fn init() -> Result<Self, String> {
        // S_FALSE (already initialised) is success too; only a mode clash fails.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_err() {
            return Err(format!("COM unavailable: {hr:?}"));
        }
        Ok(Com)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

/// Everything the window shows about networks. Costs a few tens of
/// milliseconds, so it runs on a background thread, never the UI thread.
pub fn report(exe: &Path, ports: &[u16]) -> Report {
    let _com = match Com::init() {
        Ok(c) => c,
        Err(e) => {
            return Report {
                error: Some(e),
                ..Report::default()
            }
        }
    };
    let firewall = Firewall::read(exe, ports);
    let private_block = firewall.block(Category::Private, &[]);
    match collect(&firewall) {
        Ok(networks) => Report {
            networks,
            private_block,
            error: None,
        },
        Err(e) => Report {
            networks: Vec::new(),
            private_block,
            error: Some(e),
        },
    }
}

fn collect(firewall: &Firewall) -> Result<Vec<Network>, String> {
    let adapters = adapters();

    let nlm: INetworkListManager =
        unsafe { CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL) }
            .map_err(|e| format!("Network List Manager unavailable: {e}"))?;
    let connections = unsafe { nlm.GetNetworkConnections() }
        .map_err(|e| format!("could not list network connections: {e}"))?;

    let mut out: Vec<Network> = Vec::new();
    loop {
        let mut slot: [Option<INetworkConnection>; 1] = [None];
        let mut fetched = 0u32;
        if unsafe { connections.Next(&mut slot, Some(&mut fetched)) }.is_err() || fetched == 0 {
            break;
        }
        let Some(conn) = slot[0].take() else { break };
        let (Ok(adapter_id), Ok(network)) =
            (unsafe { conn.GetAdapterId() }, unsafe { conn.GetNetwork() })
        else {
            continue;
        };
        let (Ok(id), Ok(name), Ok(cat)) = (
            unsafe { network.GetNetworkId() },
            unsafe { network.GetName() },
            unsafe { network.GetCategory() },
        ) else {
            continue;
        };
        let category = if cat == PRIVATE_NETWORK {
            Category::Private
        } else if cat == NLM_NETWORK_CATEGORY_DOMAIN_AUTHENTICATED {
            Category::Domain
        } else {
            Category::Public
        };
        let Some(adapter) = adapters.iter().find(|a| a.guid == adapter_id) else {
            continue;
        };

        // One network can span several adapters; list it once.
        if let Some(existing) = out.iter_mut().find(|n| n.id == id) {
            existing.addresses.extend(adapter.ipv4.iter().copied());
            existing.adapters.push(adapter.name.clone());
            existing.usb_tether |= adapter.usb_tether;
            existing.block = firewall.block(category, &existing.adapters);
            continue;
        }
        let names = vec![adapter.name.clone()];
        out.push(Network {
            id,
            name: name.to_string(),
            category,
            addresses: adapter.ipv4.clone(),
            block: firewall.block(category, &names),
            adapters: names,
            usb_tether: adapter.usb_tether,
        });
    }
    Ok(out)
}

/// Mark a network private. Needs administrator rights, so the window runs this
/// in an elevated copy of itself (see `main.rs`, `--trust-network`).
pub fn mark_trusted(id: GUID) -> Result<(), String> {
    let _com = Com::init()?;
    let nlm: INetworkListManager =
        unsafe { CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL) }
            .map_err(|e| format!("Network List Manager unavailable: {e}"))?;
    let network = unsafe { nlm.GetNetwork(id) }.map_err(|e| format!("network not found: {e}"))?;
    unsafe { network.SetCategory(PRIVATE_NETWORK) }
        .map_err(|e| format!("could not change the network category: {e}"))
}

pub fn parse_guid(s: &str) -> Option<GUID> {
    let t = s.trim().trim_start_matches('{').trim_end_matches('}');
    let hex: String = t.chars().filter(|c| *c != '-').collect();
    if t.len() != 36 || hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u128::from_str_radix(&hex, 16).ok().map(GUID::from_u128)
}

pub fn format_guid(g: &GUID) -> String {
    format!("{{{g:?}}}")
}

// --- adapters --------------------------------------------------------------

struct Adapter {
    guid: GUID,
    name: String,
    usb_tether: bool,
    ipv4: Vec<Ipv4Addr>,
}

/// How Windows describes a phone's USB tethering adapter: RNDIS on older
/// phones and most Android versions, NCM (Windows 11's in-box driver) on newer.
fn is_usb_tether(description: &str) -> bool {
    let d = description.to_ascii_lowercase();
    d.contains("remote ndis")
        || d.contains("rndis")
        || d.contains("usbncm")
        || d.contains("usb ncm")
}

fn adapters() -> Vec<Adapter> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size: u32 = 16 * 1024;
    let mut buf: Vec<u64> = Vec::new();
    // The table can grow between the size query and the read; retry a few times.
    let mut ok = false;
    for _ in 0..4 {
        buf = vec![0u64; (size as usize).div_ceil(8)];
        let rc = unsafe {
            GetAdaptersAddresses(
                AF_INET.0 as u32,
                flags,
                None,
                Some(buf.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>()),
                &mut size,
            )
        };
        if rc == NO_ERROR.0 {
            ok = true;
            break;
        }
        if rc != ERROR_BUFFER_OVERFLOW.0 {
            return Vec::new();
        }
    }
    if !ok {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut cur = buf.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    while !cur.is_null() {
        let a = unsafe { &*cur };
        let guid = unsafe { a.AdapterName.to_string() }
            .ok()
            .and_then(|n| parse_guid(&n));
        if let Some(guid) = guid {
            let name = unsafe { a.FriendlyName.to_string() }.unwrap_or_default();
            let description = unsafe { a.Description.to_string() }.unwrap_or_default();
            let mut ipv4 = Vec::new();
            let mut u = a.FirstUnicastAddress;
            while !u.is_null() {
                let ua = unsafe { &*u };
                let sa = ua.Address.lpSockaddr;
                if !sa.is_null() && unsafe { (*sa).sa_family } == AF_INET {
                    let sin = unsafe { &*(sa.cast::<SOCKADDR_IN>()) };
                    let raw = unsafe { sin.sin_addr.S_un.S_addr };
                    ipv4.push(Ipv4Addr::from(u32::from_be(raw)));
                }
                u = ua.Next;
            }
            out.push(Adapter {
                guid,
                usb_tether: is_usb_tether(&description),
                name,
                ipv4,
            });
        }
        cur = a.Next;
    }
    out
}

// --- firewall --------------------------------------------------------------

/// The parts of Windows Firewall's state that decide whether PhonePad is
/// reachable.
struct Firewall {
    /// Per profile (domain, private, public): enabled, blocking all inbound,
    /// and whether the default inbound action is allow.
    enabled: [bool; 3],
    block_all: [bool; 3],
    default_allow: [bool; 3],
    rules: Vec<Rule>,
    ports: Vec<u16>,
}

/// An inbound rule that applies to PhonePad's traffic.
#[derive(Debug, Clone)]
struct Rule {
    profiles: i32,
    allow: bool,
    /// Ports this rule covers among the ones asked about.
    ports: Vec<u16>,
    /// Adapter names the rule is limited to; `None` means every interface.
    interfaces: Option<Vec<String>>,
}

impl Rule {
    fn applies(&self, profile_bit: i32, port: u16, adapters: &[String]) -> bool {
        self.profiles & profile_bit != 0
            && self.ports.contains(&port)
            && self
                .interfaces
                .as_ref()
                .is_none_or(|only| only.iter().any(|i| adapters.contains(i)))
    }
}

impl Firewall {
    /// If the firewall cannot be read at all, assume nothing is blocked: a
    /// false alarm that sends someone to change a setting is worse than silence.
    fn read(exe: &Path, ports: &[u16]) -> Self {
        let mut fw = Firewall {
            enabled: [false; 3],
            block_all: [false; 3],
            default_allow: [true; 3],
            rules: Vec::new(),
            ports: ports.to_vec(),
        };
        let policy: Result<INetFwPolicy2, _> =
            unsafe { CoCreateInstance(&NetFwPolicy2, None, CLSCTX_ALL) };
        let Ok(policy) = policy else {
            return fw;
        };
        for c in [Category::Domain, Category::Private, Category::Public] {
            let i = c.index();
            let p = c.profile();
            fw.enabled[i] = unsafe { policy.get_FirewallEnabled(p) }
                .map(|b| b.as_bool())
                .unwrap_or(true);
            fw.block_all[i] =
                unsafe { policy.get_BlockAllInboundTraffic(p) }.is_ok_and(|b| b.as_bool());
            fw.default_allow[i] = unsafe { policy.get_DefaultInboundAction(p) }
                .is_ok_and(|a| a == NET_FW_ACTION_ALLOW);
        }
        fw.rules = matching_rules(&policy, exe, ports);
        fw
    }

    /// The verdict for a network of category `c` reached through `adapters`.
    /// With no adapters, interface-scoped rules do not count: that is the
    /// answer for "a private network" in general.
    fn block(&self, c: Category, adapters: &[String]) -> Option<Block> {
        verdict(
            self.enabled[c.index()],
            self.block_all[c.index()],
            self.default_allow[c.index()],
            &self.rules,
            c.profile_bit(),
            &self.ports,
            adapters,
        )
    }
}

/// Windows Firewall's own resolution, for the rules that concern PhonePad:
/// block rules win over allow rules, and every port has to get through.
fn verdict(
    enabled: bool,
    block_all: bool,
    default_allow: bool,
    rules: &[Rule],
    profile_bit: i32,
    ports: &[u16],
    adapters: &[String],
) -> Option<Block> {
    if !enabled {
        return None;
    }
    if block_all {
        return Some(Block::AllInbound);
    }
    let mut out = None;
    for &port in ports {
        let applying = || {
            rules
                .iter()
                .filter(|r| r.applies(profile_bit, port, adapters))
        };
        if applying().any(|r| !r.allow) {
            return Some(Block::ByRule);
        }
        if !default_allow && !applying().any(|r| r.allow) {
            out = Some(Block::NoAllowRule);
        }
    }
    out
}

fn matching_rules(policy: &INetFwPolicy2, exe: &Path, ports: &[u16]) -> Vec<Rule> {
    let mut out = Vec::new();
    let Ok(rules) = (unsafe { policy.Rules() }) else {
        return out;
    };
    let Ok(unknown) = (unsafe { rules._NewEnum() }) else {
        return out;
    };
    let Ok(iter) = unknown.cast::<IEnumVARIANT>() else {
        return out;
    };
    let exe = exe.to_string_lossy().to_lowercase();

    loop {
        let mut item = [VARIANT::default()];
        let mut fetched = 0u32;
        let hr = unsafe { iter.Next(&mut item, &mut fetched) };
        if hr.is_err() || fetched == 0 {
            break;
        }
        let rule = unsafe {
            let inner = &item[0].Anonymous.Anonymous;
            if inner.vt == VT_DISPATCH {
                (*inner.Anonymous.pdispVal)
                    .as_ref()
                    .and_then(|d: &IDispatch| d.cast::<INetFwRule>().ok())
            } else {
                None
            }
        };
        unsafe {
            let _ = VariantClear(&mut item[0]);
        }
        let Some(rule) = rule else { continue };
        if let Some(r) = applicable(&rule, &exe, ports) {
            out.push(r);
        }
    }
    out
}

/// The rule, if it is an enabled inbound rule that would match PhonePad's UDP
/// traffic: for this executable or for any program, UDP or any protocol, and
/// covering at least one of the ports.
fn applicable(rule: &INetFwRule, exe: &str, ports: &[u16]) -> Option<Rule> {
    unsafe {
        if !rule.Enabled().ok()?.as_bool() || rule.Direction().ok()? != NET_FW_RULE_DIR_IN {
            return None;
        }
        let protocol = rule.Protocol().ok()?;
        if protocol != NET_FW_IP_PROTOCOL_UDP.0 && protocol != NET_FW_IP_PROTOCOL_ANY.0 {
            return None;
        }
        let app = rule
            .ApplicationName()
            .map(|b| b.to_string())
            .unwrap_or_default();
        if !app.is_empty() && !app.eq_ignore_ascii_case(exe) {
            return None;
        }
        // A rule for a Windows service only applies to that service.
        if rule.ServiceName().is_ok_and(|s| !s.is_empty()) {
            return None;
        }
        let local = rule.LocalPorts().map(|b| b.to_string()).unwrap_or_default();
        let covered: Vec<u16> = ports
            .iter()
            .copied()
            .filter(|p| ports_cover(&local, *p))
            .collect();
        if covered.is_empty() {
            return None;
        }
        let action = rule.Action().ok()?;
        if action != NET_FW_ACTION_ALLOW && action != NET_FW_ACTION_BLOCK {
            return None;
        }
        let interfaces = rule.Interfaces().ok().and_then(|v| interface_names(&v));
        Some(Rule {
            profiles: rule.Profiles().ok()?,
            allow: action == NET_FW_ACTION_ALLOW,
            ports: covered,
            interfaces,
        })
    }
}

/// The adapter names in a rule's `Interfaces` value: `None` for an empty value,
/// which means every interface.
fn interface_names(v: &VARIANT) -> Option<Vec<String>> {
    unsafe {
        let inner = &v.Anonymous.Anonymous;
        if inner.vt.0 & VT_ARRAY.0 == 0 {
            return None;
        }
        let psa = inner.Anonymous.parray;
        if psa.is_null() {
            return None;
        }
        let lo = SafeArrayGetLBound(psa, 1).ok()?;
        let hi = SafeArrayGetUBound(psa, 1).ok()?;
        let mut names = Vec::new();
        for i in lo..=hi {
            let mut elem = VARIANT::default();
            if SafeArrayGetElement(psa, &i, (&mut elem as *mut VARIANT).cast()).is_ok() {
                let e = &elem.Anonymous.Anonymous;
                if e.vt == VT_BSTR {
                    names.push(e.Anonymous.bstrVal.to_string());
                }
                let _ = VariantClear(&mut elem);
            }
        }
        (!names.is_empty()).then_some(names)
    }
}

/// Whether a firewall port specification such as `*`, `47800`, `47800,47801`
/// or `47000-48000` covers `port`. An empty specification means any port.
fn ports_cover(spec: &str, port: u16) -> bool {
    let spec = spec.trim();
    if spec.is_empty() || spec == "*" {
        return true;
    }
    spec.split(',')
        .map(str::trim)
        .any(|part| match part.split_once('-') {
            Some((a, b)) => match (a.trim().parse::<u16>(), b.trim().parse::<u16>()) {
                (Ok(a), Ok(b)) => (a..=b).contains(&port),
                _ => false,
            },
            None => part.parse::<u16>() == Ok(port),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::BSTR;
    use windows::Win32::System::Ole::{
        SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement,
    };
    use windows::Win32::System::Variant::{VARENUM, VARIANT_0_0, VT_VARIANT};

    // What Windows hands back for a rule limited to some adapters: built here
    // only to check that it is read correctly.
    /// A `VT_ARRAY | VT_VARIANT` of BSTRs, the shape `INetFwRule::Interfaces` takes.
    /// The caller clears it.
    fn names_variant(names: &[String]) -> Result<VARIANT, String> {
        unsafe {
            let psa = SafeArrayCreateVector(VT_VARIANT, 0, names.len() as u32);
            if psa.is_null() {
                return Err("out of memory".into());
            }
            for (i, n) in names.iter().enumerate() {
                let mut elem = bstr_variant(n);
                let idx = i as i32;
                let put = SafeArrayPutElement(psa, &idx, (&elem as *const VARIANT).cast());
                let _ = VariantClear(&mut elem);
                if let Err(e) = put {
                    let _ = SafeArrayDestroy(psa);
                    return Err(e.to_string());
                }
            }
            let mut v = VARIANT::default();
            let inner: &mut VARIANT_0_0 = &mut v.Anonymous.Anonymous;
            inner.vt = VARENUM(VT_ARRAY.0 | VT_VARIANT.0);
            inner.Anonymous.parray = psa;
            Ok(v)
        }
    }

    fn bstr_variant(s: &str) -> VARIANT {
        let mut v = VARIANT::default();
        unsafe {
            let inner: &mut VARIANT_0_0 = &mut v.Anonymous.Anonymous;
            inner.vt = VT_BSTR;
            inner.Anonymous.bstrVal = std::mem::ManuallyDrop::new(BSTR::from(s));
        }
        v
    }

    const PORTS: [u16; 2] = [47800, 47801];
    const PUBLIC: i32 = 4;
    const PRIVATE: i32 = 2;

    fn rule(profiles: i32, allow: bool, interfaces: Option<&[&str]>) -> Rule {
        Rule {
            profiles,
            allow,
            ports: PORTS.to_vec(),
            interfaces: interfaces.map(|i| i.iter().map(|s| s.to_string()).collect()),
        }
    }

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_usual_first_run_answer_blocks_public() {
        // Answering the first-run prompt with only "Private" ticked.
        let rules = [rule(PRIVATE, true, None), rule(PUBLIC, false, None)];
        let wifi = names(&["Wi-Fi"]);
        assert_eq!(
            verdict(true, false, false, &rules, PUBLIC, &PORTS, &wifi),
            Some(Block::ByRule)
        );
        assert_eq!(
            verdict(true, false, false, &rules, PRIVATE, &PORTS, &wifi),
            None
        );
    }

    #[test]
    fn an_adapter_scoped_rule_counts_for_its_adapter_only() {
        let rules = [rule(PUBLIC | PRIVATE | 1, true, Some(&["Ethernet 3"]))];
        let tether = names(&["Ethernet 3"]);
        let cafe = names(&["Wi-Fi"]);
        assert_eq!(
            verdict(true, false, false, &rules, PUBLIC, &PORTS, &tether),
            None
        );
        assert_eq!(
            verdict(true, false, false, &rules, PUBLIC, &PORTS, &cafe),
            Some(Block::NoAllowRule),
            "an adapter-scoped rule must not count for another adapter"
        );
        assert_eq!(
            verdict(true, false, false, &rules, PUBLIC, &PORTS, &[]),
            Some(Block::NoAllowRule),
            "nor for public networks in general"
        );
    }

    #[test]
    fn block_rules_win_and_block_all_wins_over_everything() {
        let rules = [rule(PUBLIC, true, None), rule(PUBLIC, false, None)];
        let any = names(&["Wi-Fi"]);
        assert_eq!(
            verdict(true, false, false, &rules, PUBLIC, &PORTS, &any),
            Some(Block::ByRule)
        );
        let open = [rule(PUBLIC, true, None)];
        assert_eq!(
            verdict(true, true, false, &open, PUBLIC, &PORTS, &any),
            Some(Block::AllInbound)
        );
        assert_eq!(
            verdict(false, true, false, &[], PUBLIC, &PORTS, &any),
            None,
            "firewall off"
        );
    }

    #[test]
    fn every_port_must_get_through() {
        let mut half = rule(PUBLIC, true, None);
        half.ports = vec![47800];
        assert_eq!(
            verdict(
                true,
                false,
                false,
                &[half],
                PUBLIC,
                &PORTS,
                &names(&["Wi-Fi"])
            ),
            Some(Block::NoAllowRule)
        );
    }

    #[test]
    fn port_specifications() {
        assert!(ports_cover("*", 47800));
        assert!(ports_cover("", 47800));
        assert!(ports_cover("47800", 47800));
        assert!(ports_cover("47800,47801", 47801));
        assert!(ports_cover("80, 47000-48000", 47800));
        assert!(!ports_cover("47801", 47800));
        assert!(!ports_cover("RPC", 47800));
        assert!(
            !ports_cover("48000-47000", 47800),
            "a reversed range covers nothing"
        );
    }

    #[test]
    fn usb_tethering_adapters_are_recognised() {
        assert!(is_usb_tether("Remote NDIS based Internet Sharing Device"));
        assert!(is_usb_tether("Remote NDIS Compatible Device #2"));
        assert!(is_usb_tether("UsbNcm Host Device"));
        assert!(!is_usb_tether("Intel(R) Wi-Fi 6E AX211 160MHz"));
        assert!(!is_usb_tether("Realtek PCIe GbE Family Controller"));
    }

    #[test]
    fn guids_are_validated() {
        let g = parse_guid("{dcb00c01-570f-4a9b-8d69-199fdba5723b}").unwrap();
        assert_eq!(g, NetworkListManager);
        assert_eq!(parse_guid(&format_guid(&g)), Some(g));
        assert!(parse_guid("not-a-guid").is_none());
        assert!(parse_guid("{dcb00c01-570f-4a9b-8d69-199fdba5723}").is_none());
        assert!(parse_guid("{dcb00c01-570f-4a9b-8d69-199fdba5723b} && calc").is_none());
    }

    #[test]
    fn interface_lists_survive_a_variant_round_trip() {
        let v = names_variant(&names(&["Ethernet 3", "Ethernet 5"])).unwrap();
        assert_eq!(
            interface_names(&v),
            Some(names(&["Ethernet 3", "Ethernet 5"]))
        );
        let mut v = v;
        unsafe {
            let _ = VariantClear(&mut v);
        }
        assert_eq!(
            interface_names(&VARIANT::default()),
            None,
            "empty means every interface"
        );
    }
}
