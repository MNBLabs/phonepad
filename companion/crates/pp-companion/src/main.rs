//! PhonePad Windows companion.
//!
//! The window is a *reader* of `pp-core`'s shared state. It never sits in the
//! input path — if this UI froze completely, the controller would keep working.

// No console window behind the GUI in release builds. Debug keeps it so that
// panics and logs are visible while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod brand;
mod elevate;
mod firewall;
mod netprofile;
mod sponsor;

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
use pp_core::backend::BackendKind;
use pp_core::log::Level;
use pp_core::server::{Command, Core, PairOutcome};
use pp_core::store;

use brand::{BAD, GOOD, INK, INK2 as DIM, WARN};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> eframe::Result {
    // The elevated copy started by "Trust this network" does that one thing
    // and exits, before any window or socket exists.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(elevate::TRUST_NETWORK) {
        std::process::exit(trust_network(args.get(2).map(String::as_str)));
    }

    let title = format!("PhonePad Companion {VERSION}");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([880.0, 760.0])
            .with_min_inner_size([620.0, 520.0])
            .with_title(&title)
            .with_icon(Arc::new(brand::icon())),
        ..Default::default()
    };

    eframe::run_native(
        "PhonePad Companion",
        options,
        Box::new(|cc| {
            brand::install(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}

/// Runs elevated. Marks one network private and makes sure PhonePad's
/// private-network rule exists, because on a PC where PhonePad was only ever
/// allowed on public networks, making the network private would otherwise
/// trade one block for another. Exit codes: 0 done, 2 bad argument, 3 the
/// category could not be changed, 4 the rule could not be added.
fn trust_network(arg: Option<&str>) -> i32 {
    let Some(id) = arg.and_then(netprofile::parse_guid) else {
        return 2;
    };
    if netprofile::mark_trusted(id).is_err() {
        return 3;
    }
    let (config, _) = store::load_from(&store::config_path());
    match firewall::add_rule_elevated(config.discovery_port, config.input_port) {
        Ok(()) => 0,
        Err(_) => 4,
    }
}

/// What a "Trust this network" click is doing, read by the window each frame.
#[derive(Clone)]
enum TrustJob {
    Running(String),
    Done(String),
    Failed(String),
}

struct App {
    core: Option<Core>,
    show_rejects: bool,
    /// Set once the close button has been redirected to a minimise, so the
    /// reason can be shown rather than the window just refusing to close.
    minimised_note: bool,
    /// Refreshed by a background thread every few seconds; see netprofile.rs.
    networks: Arc<Mutex<netprofile::Report>>,
    /// Ask that thread for a fresh report now, e.g. right after a change.
    refresh_networks: Sender<()>,
    trust: Arc<Mutex<Option<TrustJob>>>,
    sponsor: sponsor::Strip,
}

impl App {
    fn new() -> Self {
        let core = Core::start();
        let (discovery, input) = core
            .shared
            .config
            .lock()
            .map(|c| (c.discovery_port, c.input_port))
            .unwrap_or((pp_protocol::DISCOVERY_PORT, pp_protocol::INPUT_PORT));

        let networks = Arc::new(Mutex::new(netprofile::Report::default()));
        let (refresh_networks, wake) = mpsc::channel::<()>();
        {
            let networks = Arc::clone(&networks);
            let exe = std::env::current_exe().unwrap_or_default();
            // A network can be joined or reclassified at any moment, so this
            // keeps looking; it is a reader on its own thread, like the window.
            let _ = std::thread::Builder::new()
                .name("pp-networks".into())
                .spawn(move || loop {
                    let report = netprofile::report(&exe, &[discovery, input]);
                    if let Ok(mut n) = networks.lock() {
                        *n = report;
                    }
                    match wake.recv_timeout(Duration::from_secs(4)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                });
        }

        Self {
            core: Some(core),
            show_rejects: false,
            minimised_note: false,
            networks,
            refresh_networks,
            trust: Arc::new(Mutex::new(None)),
            sponsor: sponsor::Strip::start(format!("PhonePad-Companion/{VERSION}")),
        }
    }

    fn core(&self) -> &Core {
        self.core.as_ref().expect("core is present until shutdown")
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 10 Hz is plenty for reading numbers and keeps the UI near-idle.
        ctx.request_repaint_after(Duration::from_millis(100));

        let shared = self.core().shared.clone();
        let status = shared.status_snapshot();
        let stats = shared.stats.snapshot();

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                brand::mark(ui.painter(), rect.min, 22.0, INK);
                ui.add_space(2.0);
                ui.label(
                    RichText::new("PhonePad")
                        .family(brand::display())
                        .size(22.0)
                        .color(INK),
                );
                ui.label(RichText::new("companion").color(DIM).size(15.0));
            });
            ui.add_space(16.0);

            // A sentence, not a status token. Someone who has just installed
            // this needs to know whether it is working and what to do if not;
            // "STALLED" answers neither.
            let (headline, detail, colour) = if !status.backend_ready {
                (
                    "No controller driver",
                    "Install ViGEmBus, then restart PhonePad. See Virtual controller below.",
                    BAD,
                )
            } else if stats.neutralised {
                (
                    "Controls released",
                    "The phone stopped sending. Everything is held at neutral until it comes back.",
                    WARN,
                )
            } else if stats.connected {
                (
                    "Connected",
                    "Windows is receiving your controller. You can leave this window minimised.",
                    GOOD,
                )
            } else {
                (
                    "Waiting for your phone",
                    "Open PhonePad on your phone and tap this PC. Both need to be on the same \
                     Wi-Fi, or this PC on the phone's hotspot or USB tethering.",
                    DIM,
                )
            };

            ui.horizontal(|ui| {
                // Painted rather than a bullet character: egui's bundled font
                // has no U+25CF and renders it as a missing-glyph box.
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 5.0, colour);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(headline)
                            .family(brand::display())
                            .color(colour)
                            .size(19.0),
                    );
                    ui.label(RichText::new(detail).color(DIM));
                });
            });
            ui.add_space(14.0);
        });

        // Closing the window unplugs the virtual pad. With a phone connected
        // that is almost never what the X button was meant to do — it is a
        // controller disappearing mid-game. Minimise instead, and leave an
        // explicit way to actually quit.
        if ctx.input(|i| i.viewport().close_requested()) && stats.connected {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            self.minimised_note = true;
        }

        // The sponsor strip has its own panel, added before the central one,
        // so the layout reserves its height and the content above scrolls
        // instead of ever running underneath it.
        self.sponsor_strip(ctx);

        let body = egui::Frame::new()
            .fill(brand::PAPER)
            .inner_margin(egui::Margin::symmetric(20, 12));
        egui::CentralPanel::default().frame(body).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.pairing_section(ui, &shared);
                ui.add_space(20.0);
                if self.blocked_section(ui, &shared) {
                    ui.add_space(20.0);
                }
                self.connection_section(ui, &status, &stats);
                ui.add_space(20.0);
                self.devices_section(ui, &shared);
                ui.add_space(20.0);
                self.virtual_controller_section(ui, &status);
                ui.add_space(20.0);

                if self.minimised_note {
                    ui.label(
                        RichText::new(
                            "PhonePad stays running while a phone is connected, so \
                             closing this window only hides it. Use Disconnect, then \
                             close, to stop it.",
                        )
                        .color(DIM)
                        .small(),
                    );
                    ui.add_space(12.0);
                }

                // Diagnostics, network and the log are read when something is
                // wrong. Everything above is acted on.
                brand::hairline(ui);
                ui.add_space(8.0);
                let advanced = RichText::new("Advanced")
                    .family(brand::display())
                    .size(16.0)
                    .color(INK);
                egui::CollapsingHeader::new(advanced)
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.add_space(8.0);
                        self.diagnostics_section(ui, &stats);
                        ui.add_space(20.0);
                        self.network_section(ui, &status, &shared);
                        ui.add_space(20.0);
                        self.log_section(ui, &shared);
                    });
            });
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Explicit shutdown so the virtual pad is unplugged and released rather
        // than left to process teardown.
        if let Some(core) = self.core.take() {
            core.shutdown();
        }
    }
}

fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    brand::section(ui, title, body);
}

fn kv(ui: &mut egui::Ui, key: &str, value: impl Into<String>, colour: Color32) {
    ui.label(RichText::new(key).color(DIM));
    ui.label(RichText::new(value.into()).color(colour).monospace());
    ui.end_row();
}

impl App {
    fn virtual_controller_section(&mut self, ui: &mut egui::Ui, status: &pp_core::Status) {
        card(ui, "Virtual controller", |ui| {
            ui.horizontal(|ui| {
                ui.label("Backend");
                let current = status.backend_kind.unwrap_or(BackendKind::Xbox360);
                egui::ComboBox::from_id_salt("backend")
                    .selected_text(current.label())
                    .show_ui(ui, |ui| {
                        for kind in BackendKind::ALL {
                            if ui.selectable_label(kind == current, kind.label()).clicked()
                                && kind != current
                            {
                                self.core().send(Command::SetBackend(kind));
                            }
                        }
                    });

                if status.backend_ready {
                    match status.user_index {
                        Some(i) => ui.label(RichText::new(format!("XInput slot {i}")).color(GOOD)),
                        None => ui.label(RichText::new("ready").color(GOOD)),
                    };
                }
            });

            if let Some(err) = &status.backend_error {
                ui.add_space(6.0);
                ui.colored_label(BAD, err);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Re-attach controller")
                    .on_hover_text(
                        "Unplugs the virtual pad and plugs it straight back in.\n\n\
                         Browser games and cloud-gaming streams usually bind a \
                         controller only when they see it *arrive*. If a stream \
                         was started before the pad existed, it can end up with \
                         no controller attached even though the page can read \
                         the pad fine. This produces a real arrival event without \
                         disturbing the phone.",
                    )
                    .clicked()
                {
                    self.core().send(Command::ReattachPad);
                }
                ui.label(
                    RichText::new("if a game or browser stream isn't seeing the pad")
                        .color(DIM)
                        .small(),
                );
            });

            ui.add_space(8.0);
            let mut auto = self
                .core()
                .shared
                .config
                .lock()
                .map(|c| c.auto_reattach)
                .unwrap_or(false);
            if ui
                .checkbox(&mut auto, "Do this automatically when a cloud game starts")
                .on_hover_text(
                    "Watches for the Xbox app, GeForce NOW or a browser starting, \
                     and re-attaches once, three seconds later.\n\n\
                     Off by default because it is a guess based on which programs \
                     are running. It never fires while nothing is connected, and \
                     never twice for one launch.",
                )
                .changed()
            {
                if let Ok(mut c) = self.core().shared.config.lock() {
                    c.auto_reattach = auto;
                    let snapshot = c.clone();
                    drop(c);
                    if let Err(e) =
                        pp_core::store::save_to(&self.core().shared.config_path, &snapshot)
                    {
                        self.core()
                            .shared
                            .log(pp_core::log::Level::Warn, format!("could not save: {e}"));
                    }
                }
            }
        });
    }

    fn connection_section(
        &mut self,
        ui: &mut egui::Ui,
        status: &pp_core::Status,
        stats: &pp_core::StatsSnapshot,
    ) {
        card(ui, "Connection", |ui| {
            egui::Grid::new("conn")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    kv(
                        ui,
                        "Phone",
                        status
                            .active_device_name
                            .clone()
                            .unwrap_or_else(|| "none".into()),
                        if stats.connected { GOOD } else { DIM },
                    );
                    kv(
                        ui,
                        "Address",
                        status
                            .active_device_addr
                            .clone()
                            .unwrap_or_else(|| "none".into()),
                        DIM,
                    );
                    kv(ui, "Transport", "UDP, local network", DIM);
                });

            if stats.connected {
                ui.add_space(6.0);
                if ui.button("Disconnect").clicked() {
                    self.core().send(Command::DisconnectSession);
                }
            }
        });
    }

    fn diagnostics_section(&mut self, ui: &mut egui::Ui, s: &pp_core::StatsSnapshot) {
        card(ui, "Diagnostics", |ui| {
            egui::Grid::new("diag")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    kv(
                        ui,
                        "Packet rate",
                        format!("{} /sec", s.pps),
                        if s.pps > 0 { GOOD } else { DIM },
                    );
                    let loss = s.loss_permille as f32 / 10.0;
                    kv(
                        ui,
                        "Packet loss",
                        format!("{loss:.1} %"),
                        if loss < 1.0 {
                            GOOD
                        } else if loss < 5.0 {
                            WARN
                        } else {
                            BAD
                        },
                    );
                    kv(
                        ui,
                        "Jitter",
                        format!("{:.2} ms", s.jitter_us as f32 / 1000.0),
                        DIM,
                    );
                    let rtt = s.rtt_us as f32 / 1000.0;
                    kv(
                        ui,
                        "Round trip (phone)",
                        if s.rtt_us == 0 {
                            "none".to_string()
                        } else {
                            format!("{rtt:.1} ms")
                        },
                        if rtt < 10.0 {
                            GOOD
                        } else if rtt < 25.0 {
                            WARN
                        } else {
                            BAD
                        },
                    );
                    kv(ui, "Accepted", s.accepted.to_string(), DIM);
                    kv(
                        ui,
                        "Rejected",
                        s.total_rejected().to_string(),
                        if s.total_rejected() == 0 { DIM } else { WARN },
                    );
                });

            ui.add_space(4.0);
            ui.checkbox(&mut self.show_rejects, "Show rejection breakdown");
            if self.show_rejects {
                ui.add_space(4.0);
                egui::Grid::new("rej")
                    .num_columns(2)
                    .spacing([16.0, 2.0])
                    .show(ui, |ui| {
                        kv(ui, "Malformed", s.rejected_malformed.to_string(), DIM);
                        kv(ui, "Bad MAC", s.rejected_mac.to_string(), DIM);
                        kv(ui, "Replayed", s.rejected_replay.to_string(), DIM);
                        kv(ui, "Bad field", s.rejected_field.to_string(), DIM);
                        kv(
                            ui,
                            "Unknown session",
                            s.rejected_unknown_session.to_string(),
                            DIM,
                        );
                        kv(ui, "Sessions started", s.sessions_started.to_string(), DIM);
                        kv(ui, "Sessions dropped", s.sessions_dropped.to_string(), DIM);
                        kv(ui, "Neutralisations", s.neutralisations.to_string(), DIM);
                        kv(
                            ui,
                            "Backend errors",
                            s.backend_errors.to_string(),
                            if s.backend_errors == 0 { DIM } else { BAD },
                        );
                    });
            }
        });
    }

    fn pairing_section(&mut self, ui: &mut egui::Ui, shared: &pp_core::Shared) {
        card(ui, "Pairing", |ui| {
            let Ok(mut pairing) = shared.pairing.lock() else {
                ui.colored_label(BAD, "pairing state unavailable");
                return;
            };

            if pairing.active {
                brand::raised(ui, |ui| {
                    ui.label(RichText::new("Enter this code on the phone").color(DIM));
                    ui.add_space(2.0);
                    // Spaced digits in the display face: it is read across a
                    // room and typed on another device.
                    let spaced: String = pairing.code.chars().map(|c| format!("{c} ")).collect();
                    ui.label(
                        RichText::new(spaced.trim_end())
                            .family(brand::display())
                            .color(INK)
                            .size(40.0),
                    );
                    if let Some(left) = pairing.remaining() {
                        ui.label(
                            RichText::new(format!("Expires in {} s", left.as_secs())).color(DIM),
                        );
                    }
                });
                // A code is used up by every attempt, right or wrong, so after
                // a failure the one on screen is new. Say so.
                match &pairing.last_outcome {
                    Some(PairOutcome::Refused(name)) => {
                        ui.add_space(4.0);
                        ui.colored_label(
                            WARN,
                            format!("{name} entered the wrong code. Use the new one above."),
                        );
                    }
                    Some(PairOutcome::Outdated(name)) => {
                        ui.add_space(4.0);
                        ui.colored_label(
                            WARN,
                            format!(
                                "{name} has an older PhonePad that pairs insecurely. Update \
                                 the app on the phone, then pair again."
                            ),
                        );
                    }
                    _ => {}
                }
                ui.add_space(6.0);
                if ui.button("Cancel pairing").clicked() {
                    pairing.end();
                }
            } else {
                if brand::primary(ui, "Pair a phone").clicked() {
                    pairing.begin();
                }
                match &pairing.last_outcome {
                    Some(PairOutcome::Paired(name)) => {
                        ui.colored_label(GOOD, format!("Paired with {name}."));
                    }
                    Some(PairOutcome::Refused(name)) => {
                        ui.colored_label(WARN, format!("{name} entered the wrong code."));
                    }
                    Some(PairOutcome::Outdated(name)) => {
                        ui.colored_label(
                            WARN,
                            format!(
                                "{name} has an older PhonePad that pairs insecurely. \
                                 Update the app on the phone, then pair again."
                            ),
                        );
                    }
                    Some(PairOutcome::TooManyAttempts) => {
                        ui.colored_label(
                            WARN,
                            "Pairing stopped after several wrong codes. If that wasn't \
                             you, something else on this network tried to pair.",
                        );
                    }
                    None => {
                        ui.label(
                            RichText::new("Open PhonePad on the phone, then click here.")
                                .color(DIM),
                        );
                    }
                }
            }
        });
    }

    fn devices_section(&mut self, ui: &mut egui::Ui, shared: &pp_core::Shared) {
        card(ui, "Trusted phones", |ui| {
            let Ok(mut cfg) = shared.config.lock() else {
                return;
            };

            if cfg.devices.is_empty() {
                ui.label(RichText::new("None yet. Pair a phone to get started.").color(DIM));
                return;
            }

            let mut forget: Option<String> = None;
            brand::raised(ui, |ui| {
                for (i, d) in cfg.devices.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(4.0);
                        brand::hairline(ui);
                        ui.add_space(4.0);
                    }
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&d.name).family(brand::semibold()));
                        ui.label(
                            RichText::new(d.last_seen.as_deref().unwrap_or("never connected"))
                                .color(DIM)
                                .small(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Forget").clicked() {
                                forget = Some(d.id.clone());
                            }
                        });
                    });
                }
            });

            if let Some(id) = forget {
                cfg.forget_device(&id);
                if let Err(e) = store::save_to(&shared.config_path, &cfg) {
                    shared.log(Level::Error, format!("could not save config: {e}"));
                } else {
                    shared.log(Level::Info, "forgot a paired phone");
                }
            }
        });
    }

    fn network_section(
        &mut self,
        ui: &mut egui::Ui,
        status: &pp_core::Status,
        shared: &pp_core::Shared,
    ) {
        card(ui, "Network", |ui| {
            egui::Grid::new("net")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    kv(
                        ui,
                        "Discovery",
                        status
                            .discovery_bound
                            .clone()
                            .unwrap_or_else(|| "not bound".into()),
                        if status.discovery_bound.is_some() {
                            GOOD
                        } else {
                            BAD
                        },
                    );
                    kv(
                        ui,
                        "Input",
                        status
                            .input_bound
                            .clone()
                            .unwrap_or_else(|| "not bound".into()),
                        if status.input_bound.is_some() {
                            GOOD
                        } else {
                            BAD
                        },
                    );
                    kv(
                        ui,
                        "Phones searching",
                        match &status.last_discovery {
                            Some(last) => format!(
                                "{} probes, last: {last}",
                                shared.stats.snapshot().discovery_requests
                            ),
                            None => "none heard yet".into(),
                        },
                        DIM,
                    );
                    let report = self.networks.lock().map(|r| r.clone()).unwrap_or_default();
                    for n in &report.networks {
                        let ips: Vec<String> = n.addresses.iter().map(|a| a.to_string()).collect();
                        let verdict = if n.block.is_some() { "blocked" } else { "open" };
                        kv(
                            ui,
                            &n.name,
                            format!(
                                "{} · {} · PhonePad {verdict}",
                                if ips.is_empty() {
                                    "no IPv4".into()
                                } else {
                                    ips.join(", ")
                                },
                                n.category.label()
                            ),
                            if n.block.is_some() { WARN } else { DIM },
                        );
                    }
                    if let Some(e) = &report.error {
                        kv(ui, "Networks", e.clone(), WARN);
                    }
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Name shown on the phone");
                let mut name = shared
                    .config
                    .lock()
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let computer = std::env::var("COMPUTERNAME").unwrap_or_default();
                let field = egui::TextEdit::singleline(&mut name)
                    .hint_text(computer)
                    .desired_width(220.0);
                if ui.add(field).lost_focus() {
                    if let Ok(mut c) = shared.config.lock() {
                        if c.name != name {
                            c.name = name;
                            let snapshot = c.clone();
                            drop(c);
                            if let Err(e) = store::save_to(&shared.config_path, &snapshot) {
                                shared.log(Level::Warn, format!("could not save: {e}"));
                            }
                        }
                    }
                }
            });

            if let Some(err) = &status.bind_error {
                ui.add_space(6.0);
                ui.colored_label(BAD, err);
            }
        });
    }

    fn log_section(&mut self, ui: &mut egui::Ui, shared: &pp_core::Shared) {
        card(ui, "Log", |ui| {
            let Ok(mut log) = shared.log.lock() else {
                return;
            };

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} messages", log.total))
                        .color(DIM)
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Clear").clicked() {
                        log.clear();
                    }
                });
            });
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .max_height(220.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in log.iter() {
                        let colour = match line.level {
                            Level::Info => DIM,
                            Level::Warn => WARN,
                            Level::Error => BAD,
                        };
                        ui.label(
                            RichText::new(format!("{} {}", line.at, line.text))
                                .color(colour)
                                .monospace()
                                .small(),
                        );
                    }
                });
        });
    }
}

impl App {
    /// Shown only when Windows Firewall would stop a phone reaching PhonePad
    /// on a network this PC is on. Returns whether anything was drawn.
    fn blocked_section(&mut self, ui: &mut egui::Ui, shared: &pp_core::Shared) -> bool {
        let report = self.networks.lock().map(|r| r.clone()).unwrap_or_default();
        let trusting_works = report.private_would_work();
        let blocked: Vec<netprofile::Network> = report
            .networks
            .into_iter()
            .filter(|n| n.block.is_some())
            .collect();
        let job = self.trust.lock().ok().and_then(|j| j.clone());
        if blocked.is_empty() && !matches!(job, Some(TrustJob::Done(_))) {
            return false;
        }
        let busy = matches!(job, Some(TrustJob::Running(_)));

        // Whatever blocks a public network (no rule at all, or the block rule
        // Windows writes when only "Private" was ticked at its first-run
        // prompt), the fix for the user's own hotspot is the same: make that
        // one network private. Offered only when private networks are not
        // themselves blocked by something adding a rule cannot get past.
        let trustable = |n: &netprofile::Network| {
            n.category == netprofile::Category::Public
                && matches!(
                    n.block,
                    Some(netprofile::Block::NoAllowRule | netprofile::Block::ByRule)
                )
                && trusting_works
        };

        card(ui, "Windows is blocking PhonePad", |ui| {
            if let Some(TrustJob::Done(msg)) = &job {
                ui.colored_label(GOOD, msg);
                if blocked.is_empty() {
                    return;
                }
                ui.add_space(10.0);
            }

            if blocked.iter().any(trustable) {
                ui.label(
                    RichText::new(
                        "Windows treats a network it has not seen before as public, and \
                         blocks phones from reaching PhonePad there. A phone's hotspot or \
                         USB tethering always starts out that way. If one of these is your \
                         own phone or your own network, make it private: Windows remembers \
                         that for that network only.",
                    )
                    .color(INK),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Don't do this for café, hotel, airport or other shared Wi-Fi.")
                        .color(WARN),
                );
                ui.add_space(10.0);
            }

            brand::raised(ui, |ui| {
                for (i, n) in blocked.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(6.0);
                        brand::hairline(ui);
                        ui.add_space(6.0);
                    }
                    self.blocked_network(ui, shared, n, trustable(n), busy);
                }
            });

            match &job {
                Some(TrustJob::Running(name)) => {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("Waiting for Windows to change \"{name}\"…"))
                            .color(DIM),
                    );
                }
                Some(TrustJob::Failed(msg)) => {
                    ui.add_space(8.0);
                    ui.colored_label(BAD, msg);
                }
                _ => {}
            }
        });
        true
    }

    /// One row: the network, what it is, and the one action that fixes it.
    fn blocked_network(
        &self,
        ui: &mut egui::Ui,
        shared: &pp_core::Shared,
        n: &netprofile::Network,
        trustable: bool,
        busy: bool,
    ) {
        let ips: Vec<String> = n.addresses.iter().map(|a| a.to_string()).collect();
        let detail = if ips.is_empty() {
            format!("{} network", n.category.label())
        } else {
            format!("{} network · {}", n.category.label(), ips.join(", "))
        };

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(&n.name).family(brand::semibold()).color(INK));
                ui.label(RichText::new(detail).color(DIM).small());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let action = if trustable {
                    Some("Make private (needs admin)")
                } else if n.block == Some(netprofile::Block::NoAllowRule) {
                    Some("Allow PhonePad (needs admin)")
                } else {
                    None
                };
                if let Some(label) = action {
                    if ui
                        .add_enabled(!busy, egui::Button::new(label))
                        .on_hover_text(if trustable {
                            "Marks this one network private and allows PhonePad's two UDP \
                             ports on private networks. Nothing changes for public networks."
                        } else {
                            "Allows PhonePad's two UDP ports on private networks only."
                        })
                        .clicked()
                    {
                        self.start_trust(shared, n);
                    }
                }
            });
        });

        let why = match n.block {
            // Seen on a Galaxy S24 Ultra: each USB tethering session arrives
            // with a new random MAC, and Windows names wired networks by
            // their gateway's MAC, so "Network 22" becomes "Network 23".
            _ if trustable && n.usb_tether => Some(
                "This is USB tethering. Many phones give it a new identity each time \
                 tethering starts, so Windows may ask again next time. A Wi-Fi hotspot \
                 is remembered.",
            ),
            _ if trustable => None,
            Some(netprofile::Block::NoAllowRule) => {
                Some("No Windows Firewall rule lets phones reach PhonePad on this network.")
            }
            Some(netprofile::Block::ByRule) => Some(
                "A Windows Firewall rule blocks PhonePad here, usually from choosing Cancel \
                 when Windows first asked about it. In Windows Security, open Firewall & \
                 network protection, then Allow an app through firewall, and tick PhonePad \
                 Companion for this kind of network.",
            ),
            Some(netprofile::Block::AllInbound) => Some(
                "Windows Firewall blocks every incoming connection on this kind of network. \
                 That setting is in Windows Security, under Firewall & network protection.",
            ),
            None => None,
        };
        if let Some(why) = why {
            ui.add_space(4.0);
            ui.label(RichText::new(why).color(DIM));
        }
    }

    /// Run the elevated helper on a thread, so the window keeps drawing while
    /// Windows shows its consent prompt.
    fn start_trust(&self, shared: &pp_core::Shared, n: &netprofile::Network) {
        let name = n.name.clone();
        let args = format!(
            "{} {}",
            elevate::TRUST_NETWORK,
            netprofile::format_guid(&n.id)
        );
        if let Ok(mut j) = self.trust.lock() {
            *j = Some(TrustJob::Running(name.clone()));
        }
        shared.log(Level::Info, format!("asking Windows to trust \"{name}\""));
        let trust = Arc::clone(&self.trust);
        let refresh = self.refresh_networks.clone();
        let _ = std::thread::Builder::new()
            .name("pp-trust".into())
            .spawn(move || {
                let outcome = match elevate::run_self_elevated(&args) {
                    Ok(0) => TrustJob::Done(format!(
                        "\"{name}\" is now a private network and PhonePad is allowed on it."
                    )),
                    Ok(3) => TrustJob::Failed(format!(
                        "Windows would not change \"{name}\". You can do it in Settings → \
                         Network & internet → this network → Network profile type → Private."
                    )),
                    Ok(4) => TrustJob::Failed(
                        "The network is private now, but the firewall rule could not be added."
                            .into(),
                    ),
                    Ok(code) => {
                        TrustJob::Failed(format!("The change did not finish (code {code})."))
                    }
                    Err(e) => TrustJob::Failed(e),
                };
                if let Ok(mut j) = trust.lock() {
                    *j = Some(outcome);
                }
                let _ = refresh.send(());
            });
    }

    /// One placement, in a strip of its own at the bottom of the window.
    fn sponsor_strip(&self, ctx: &egui::Context) {
        let p = self.sponsor.current();
        let frame = egui::Frame::new()
            .fill(brand::RAISED)
            .stroke(egui::Stroke::new(1.0_f32, brand::LINE))
            .inner_margin(egui::Margin::symmetric(20, 10));
        egui::TopBottomPanel::bottom("sponsor")
            .exact_height(58.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(frame)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(RichText::new(&p.action).color(brand::ACCENT))
                            .on_hover_text(&p.url)
                            .clicked()
                        {
                            elevate::open_https(&p.url);
                        }
                        ui.add_space(12.0);
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("{} · {}", p.label, p.title))
                                        .family(brand::semibold())
                                        .color(INK),
                                )
                                .truncate(),
                            );
                            ui.add(
                                egui::Label::new(RichText::new(&p.body).color(DIM).small())
                                    .truncate(),
                            );
                        });
                    });
                });
            });
    }
}
