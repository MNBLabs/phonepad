//! PhonePad Windows companion.
//!
//! The window is a *reader* of `pp-core`'s shared state. It never sits in the
//! input path — if this UI froze completely, the controller would keep working.

// No console window behind the GUI in release builds. Debug keeps it so that
// panics and logs are visible while developing.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod brand;
mod firewall;

use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Color32, RichText};
use pp_core::backend::BackendKind;
use pp_core::log::Level;
use pp_core::server::{Command, Core, PairOutcome};
use pp_core::store;

use brand::{BAD, GOOD, INK, INK2 as DIM, WARN};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([880.0, 720.0])
            .with_min_inner_size([620.0, 480.0])
            .with_title("PhonePad Companion")
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

struct App {
    core: Option<Core>,
    firewall: firewall::RuleState,
    firewall_note: Option<(bool, String)>,
    show_rejects: bool,
    /// Set once the close button has been redirected to a minimise, so the
    /// reason can be shown rather than the window just refusing to close.
    minimised_note: bool,
}

impl App {
    fn new() -> Self {
        Self {
            core: Some(Core::start()),
            firewall: firewall::rule_state(),
            firewall_note: None,
            show_rejects: false,
            minimised_note: false,
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
                    "Open PhonePad on your phone and tap this PC. Both need to be on the same Wi-Fi.",
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

        let body = egui::Frame::new()
            .fill(brand::PAPER)
            .inner_margin(egui::Margin::symmetric(20, 12));
        egui::CentralPanel::default().frame(body).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.pairing_section(ui, &shared);
                ui.add_space(20.0);
                self.connection_section(ui, &status, &stats);
                ui.add_space(20.0);
                self.devices_section(ui, &shared);
                ui.add_space(20.0);
                self.virtual_controller_section(ui, &status);
                ui.add_space(20.0);

                if self.minimised_note {
                    ui.label(
                        RichText::new(
                            "PhonePad stays running while a phone is connected, so                              closing this window only hides it. Use Disconnect, then                              close, to stop it.",
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
                    "Watches for the Xbox app, GeForce NOW or a browser starting,                      and re-attaches once, three seconds later.

                     Off by default because it is a guess based on which programs                      are running. It never fires while nothing is connected, and                      never twice for one launch.",
                )
                .changed()
            {
                if let Ok(mut c) = self.core().shared.config.lock() {
                    c.auto_reattach = auto;
                    let snapshot = c.clone();
                    drop(c);
                    if let Err(e) = pp_core::store::save_to(
                        &self.core().shared.config_path,
                        &snapshot,
                    ) {
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
                    kv(ui, "Transport", "Wi-Fi / UDP", DIM);
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
                    for ip in local_ipv4s() {
                        kv(ui, "This PC", ip, DIM);
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

            ui.add_space(8.0);
            match &self.firewall {
                firewall::RuleState::Present => {
                    ui.colored_label(GOOD, "Windows Firewall rule is in place.");
                }
                firewall::RuleState::Missing => {
                    ui.colored_label(
                        WARN,
                        "No firewall rule found. If the phone cannot see this PC, add one.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Add firewall rule (needs admin)").clicked() {
                            let (d, i) = shared
                                .config
                                .lock()
                                .map(|c| (c.discovery_port, c.input_port))
                                .unwrap_or((pp_protocol::DISCOVERY_PORT, pp_protocol::INPUT_PORT));
                            match firewall::add_rule(d, i) {
                                Ok(()) => {
                                    self.firewall = firewall::rule_state();
                                    self.firewall_note =
                                        Some((true, "Firewall rule added.".to_string()));
                                    shared.log(Level::Info, "added the Windows Firewall rule");
                                }
                                Err(e) => {
                                    self.firewall_note = Some((false, e.clone()));
                                    shared
                                        .log(Level::Warn, format!("firewall rule not added: {e}"));
                                }
                            }
                        }
                        if ui.button("Re-check").clicked() {
                            self.firewall = firewall::rule_state();
                        }
                    });
                }
                firewall::RuleState::Unknown(e) => {
                    ui.colored_label(WARN, format!("Could not check the firewall: {e}"));
                }
            }

            if let Some((ok, note)) = &self.firewall_note {
                ui.colored_label(if *ok { GOOD } else { BAD }, note);
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

/// Best-effort list of this machine's IPv4 addresses, shown so the user can
/// sanity-check that the PC and the phone are on the same subnet.
fn local_ipv4s() -> Vec<String> {
    use std::net::UdpSocket;
    // Connecting a UDP socket performs no traffic but makes the OS pick the
    // outbound interface, which is the address the phone will actually reach.
    let mut out = Vec::new();
    if let Ok(s) = UdpSocket::bind("0.0.0.0:0") {
        if s.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = s.local_addr() {
                out.push(addr.ip().to_string());
            }
        }
    }
    out
}
