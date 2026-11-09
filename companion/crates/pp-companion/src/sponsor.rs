//! The sponsor strip at the bottom of the window.
//!
//! PhonePad is free and pays for itself with one quiet placement in space the
//! window does not otherwise use. Programmatic ad networks are not an option on
//! the desktop: AdSense forbids ads in desktop software outright, and nothing
//! else reputable serves native Windows apps. So the strip carries a sponsor or
//! house placement chosen by hand, published as a small JSON file on the
//! PhonePad site, which can change without a release.
//!
//! What this does on the network, in full: at most one HTTPS GET of
//! `https://phonepad.dynshift.com/sponsor.json` every twelve hours, with no
//! cookies, no identifiers and no query string, through Windows' own TLS and
//! proxy settings. Nothing is reported back: not views, not clicks. A click
//! only opens the link in the browser.
//!
//! Everything fetched is treated as hostile: size-capped, strictly validated,
//! plain text only, and links must be https. If anything about it is wrong,
//! or there is no network, the strip shows the built-in placement instead, so
//! it is never empty and never broken.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use windows::core::{w, PCWSTR};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption,
    WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

const HOST: PCWSTR = w!("phonepad.dynshift.com");
const PATH: PCWSTR = w!("/sponsor.json");
/// A real placement file is a few hundred bytes.
const MAX_BYTES: usize = 16 * 1024;
const REFRESH: Duration = Duration::from_secs(12 * 60 * 60);
/// A cached file older than this is no longer trusted to be current.
const STALE: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// How long each placement stays before the next one, when there are several.
const ROTATE: Duration = Duration::from_secs(10 * 60);

/// One placement, already validated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    /// "Sponsored" or "From the maker". Disclosed on every placement.
    pub label: String,
    pub title: String,
    pub body: String,
    pub action: String,
    pub url: String,
}

/// Shown when nothing valid has been fetched: no network, a broken file, or a
/// build run before the site carried one.
pub fn house() -> Placement {
    Placement {
        label: "From the maker".into(),
        title: "PhonePad is free and open source".into(),
        body: "This strip keeps it that way. You can also chip in directly.".into(),
        action: "Support PhonePad".into(),
        url: "https://phonepad.dynshift.com/support/".into(),
    }
}

#[derive(Deserialize)]
struct Feed {
    version: u32,
    placements: Vec<Placement>,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    fetched_at: u64,
    placements: Vec<Placement>,
}

/// Parse and validate a feed. Any single invalid field drops that placement;
/// a feed with none left is rejected as a whole.
pub fn parse(bytes: &[u8]) -> Option<Vec<Placement>> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let feed: Feed = serde_json::from_slice(bytes).ok()?;
    if feed.version != 1 {
        return None;
    }
    let valid: Vec<Placement> = feed
        .placements
        .into_iter()
        .take(6)
        .filter_map(validate)
        .collect();
    (!valid.is_empty()).then_some(valid)
}

fn validate(p: Placement) -> Option<Placement> {
    let label = match p.label.trim() {
        l @ ("Sponsored" | "From the maker") => l.to_string(),
        _ => return None,
    };
    let text = |s: &str, max: usize| -> Option<String> {
        let t = s.trim();
        let ok = !t.is_empty() && t.chars().count() <= max && !t.chars().any(char::is_control);
        ok.then(|| t.to_string())
    };
    let url = p.url.trim();
    let url_ok = url.len() <= 300
        && url.starts_with("https://")
        && url.len() > "https://".len()
        && !url.chars().any(|c| c.is_control() || c.is_whitespace());
    if !url_ok {
        return None;
    }
    Some(Placement {
        label,
        title: text(&p.title, 60)?,
        body: text(&p.body, 120)?,
        action: text(&p.action, 24)?,
        url: url.to_string(),
    })
}

/// Which placement to show now. Time-based, so it rotates without keeping any
/// record of what was shown.
pub fn pick(placements: &[Placement], now: SystemTime) -> Placement {
    if placements.is_empty() {
        return house();
    }
    let slot = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / ROTATE.as_secs())
        .unwrap_or(0);
    placements[(slot % placements.len() as u64) as usize].clone()
}

/// The placements the window draws from. Starts with whatever is cached (or
/// the house placement) and is updated by a background thread.
pub struct Strip {
    placements: Arc<Mutex<Vec<Placement>>>,
}

impl Strip {
    pub fn start(user_agent: String) -> Self {
        let cached = load_cache()
            .filter(|c| age(c.fetched_at) < STALE)
            .map(|c| c.placements)
            .unwrap_or_default();
        let fresh = load_cache().is_some_and(|c| age(c.fetched_at) < REFRESH);

        let placements = Arc::new(Mutex::new(cached));
        if !fresh {
            let target = Arc::clone(&placements);
            // Never on the UI thread, and never anywhere near the input path.
            let _ = std::thread::Builder::new()
                .name("pp-sponsor".into())
                .spawn(move || {
                    if let Some(p) = fetch(&user_agent).as_deref().and_then(parse) {
                        save_cache(&p);
                        if let Ok(mut t) = target.lock() {
                            *t = p;
                        }
                    }
                });
        }
        Self { placements }
    }

    pub fn current(&self) -> Placement {
        let list = self
            .placements
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default();
        pick(&list, SystemTime::now())
    }
}

fn age(fetched_at: u64) -> Duration {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Duration::from_secs(now.saturating_sub(fetched_at))
}

fn cache_path() -> PathBuf {
    pp_core::store::config_path().with_file_name("sponsor.json")
}

fn load_cache() -> Option<Cache> {
    let bytes = std::fs::read(cache_path()).ok()?;
    let cache: Cache = serde_json::from_slice(&bytes).ok()?;
    // Re-validate: the file is on disk and could have been edited.
    let placements: Vec<Placement> = cache.placements.into_iter().filter_map(validate).collect();
    Some(Cache {
        fetched_at: cache.fetched_at,
        placements,
    })
}

fn save_cache(placements: &[Placement]) {
    let cache = Cache {
        fetched_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        placements: placements.to_vec(),
    };
    if let Ok(text) = serde_json::to_vec_pretty(&cache) {
        let _ = std::fs::write(cache_path(), text);
    }
}

/// Closes a WinHTTP handle on every path out.
struct Handle(*mut core::ffi::c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// GET the feed. `None` on any failure; the caller falls back.
fn fetch(user_agent: &str) -> Option<Vec<u8>> {
    let ua: Vec<u16> = user_agent.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let session = Handle(WinHttpOpen(
            PCWSTR(ua.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return None;
        }
        WinHttpSetTimeouts(session.0, 5_000, 5_000, 5_000, 5_000).ok()?;

        let connect = Handle(WinHttpConnect(session.0, HOST, 443, 0));
        if connect.0.is_null() {
            return None;
        }
        let request = Handle(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            PATH,
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return None;
        }
        // The file lives at one fixed https URL; a redirect means something
        // is wrong, not that it moved.
        let never = WINHTTP_OPTION_REDIRECT_POLICY_NEVER.to_le_bytes();
        WinHttpSetOption(
            Some(request.0),
            WINHTTP_OPTION_REDIRECT_POLICY,
            Some(&never),
        )
        .ok()?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).ok()?;

        let mut status = 0u32;
        let mut len = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut len,
            std::ptr::null_mut(),
        )
        .ok()?;
        if status != 200 {
            return None;
        }

        let mut body = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr().cast(),
                chunk.len() as u32,
                &mut read,
            )
            .ok()?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read as usize]);
            if body.len() > MAX_BYTES {
                return None;
            }
        }
        Some(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(placement: &str) -> Vec<u8> {
        format!(r#"{{"version":1,"placements":[{placement}]}}"#).into_bytes()
    }

    const GOOD: &str = r#"{"label":"Sponsored","title":"A title","body":"A body.","action":"Go","url":"https://example.com/x"}"#;

    #[test]
    fn a_valid_feed_parses() {
        let p = parse(&feed(GOOD)).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].label, "Sponsored");
    }

    #[test]
    fn links_must_be_https() {
        for url in [
            "http://example.com",
            "javascript:alert(1)",
            "file:///C:/Windows/System32/calc.exe",
            "https://",
            "https://exa mple.com",
            "https://example.com/\u{0007}",
        ] {
            let p = GOOD.replace("https://example.com/x", url);
            assert!(parse(&feed(&p)).is_none(), "{url} was accepted");
        }
    }

    #[test]
    fn the_label_must_disclose_what_it_is() {
        let p = GOOD.replace("Sponsored", "Recommended");
        assert!(parse(&feed(&p)).is_none());
    }

    #[test]
    fn overlong_or_control_text_is_dropped() {
        let long = GOOD.replace("A title", &"x".repeat(61));
        assert!(parse(&feed(&long)).is_none());
        let ctl = GOOD.replace("A body.", "line\\nbreak");
        assert!(parse(&feed(&ctl)).is_none());
    }

    #[test]
    fn one_bad_placement_does_not_spoil_the_rest() {
        let bad = GOOD.replace("https://example.com/x", "http://nope");
        let p = parse(&feed(&format!("{bad},{GOOD}"))).unwrap();
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn unknown_versions_and_oversized_files_are_rejected() {
        let v2 = String::from_utf8(feed(GOOD))
            .unwrap()
            .replace("\"version\":1", "\"version\":2");
        assert!(parse(v2.as_bytes()).is_none());
        assert!(parse(&vec![b' '; MAX_BYTES + 1]).is_none());
        assert!(parse(b"not json").is_none());
    }

    #[test]
    fn the_shipped_feed_is_valid() {
        let shipped = include_bytes!("../../../../site/public/sponsor.json");
        assert!(
            parse(shipped).is_some(),
            "site/public/sponsor.json does not validate"
        );
    }

    #[test]
    fn rotation_is_by_time_and_falls_back_to_the_house() {
        let a = parse(&feed(GOOD)).unwrap().remove(0);
        let mut b = a.clone();
        b.title = "Another".into();
        let both = vec![a.clone(), b.clone()];
        let t0 = UNIX_EPOCH;
        assert_eq!(pick(&both, t0), a);
        assert_eq!(pick(&both, t0 + ROTATE), b);
        assert_eq!(pick(&[], t0), house());
        assert!(
            validate(house()).is_some(),
            "the built-in placement must pass its own rules"
        );
    }
}
