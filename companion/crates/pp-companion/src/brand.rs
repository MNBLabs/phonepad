//! The design system, as far as egui can carry it. Values come from
//! `docs/design.md`; the phone has the same numbers in `theme.dart`. Two
//! processes in two languages cannot share a token file, but they are one
//! product and someone looking at both at once will notice if they disagree.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText};
use std::sync::Arc;

// --- paper -------------------------------------------------------------------

pub const PAPER: Color32 = Color32::from_rgb(0xF7, 0xF7, 0xFC);
pub const RAISED: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const SUNKEN: Color32 = Color32::from_rgb(0xEE, 0xED, 0xF7);
pub const LINE: Color32 = Color32::from_rgb(0xE2, 0xE1, 0xEE);
pub const INK: Color32 = Color32::from_rgb(0x15, 0x14, 0x2B);
pub const INK2: Color32 = Color32::from_rgb(0x5A, 0x58, 0x78);
pub const INK3: Color32 = Color32::from_rgb(0x80, 0x7E, 0xA0);
pub const BRAND: Color32 = Color32::from_rgb(0x81, 0x78, 0xFF);
pub const ACCENT: Color32 = Color32::from_rgb(0x5A, 0x4F, 0xE6);
pub const ACCENT_DEEP: Color32 = Color32::from_rgb(0x4A, 0x40, 0xD4);
pub const GOOD: Color32 = Color32::from_rgb(0x17, 0x7F, 0x4A);
pub const WARN: Color32 = Color32::from_rgb(0x93, 0x5B, 0x00);
pub const BAD: Color32 = Color32::from_rgb(0xC4, 0x38, 0x37);

pub const RADIUS_M: u8 = 14;
pub const RADIUS_S: u8 = 8;
/// egui caps a corner radius at 255, which is a pill on anything shorter.
pub const PILL: u8 = 255;

// --- type --------------------------------------------------------------------

/// Bricolage Grotesque, one family for every surface. The 12pt cut carries the
/// interface, the 96pt cut the title. Tabular figures are baked into the UI
/// cuts (see `assets/`), because egui applies no OpenType features and the
/// diagnostics would otherwise jitter as they update.
const REGULAR: &[u8] = include_bytes!("../assets/Bricolage-Regular.ttf");
const MEDIUM: &[u8] = include_bytes!("../assets/Bricolage-Medium.ttf");
const SEMIBOLD: &[u8] = include_bytes!("../assets/Bricolage-SemiBold.ttf");
const DISPLAY: &[u8] = include_bytes!("../assets/BricolageDisplay-SemiBold.ttf");

pub fn medium() -> FontFamily {
    FontFamily::Name("medium".into())
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

pub fn display() -> FontFamily {
    FontFamily::Name("display".into())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("regular", REGULAR),
        ("medium", MEDIUM),
        ("semibold", SEMIBOLD),
        ("display", DISPLAY),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
        fonts
            .families
            .insert(FontFamily::Name(name.into()), vec![name.to_owned()]);
    }
    // Regular becomes the proportional default, with egui's own fonts kept
    // behind it for any glyph Bricolage lacks.
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .expect("egui always defines a proportional family")
        .insert(0, "regular".to_owned());
    // The design has no monospace. A number is a number, and the figures are
    // tabular anyway.
    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .expect("egui always defines a monospace family")
        .insert(0, "medium".to_owned());
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (egui::TextStyle::Heading, FontId::new(22.0, display())),
        (
            egui::TextStyle::Body,
            FontId::new(14.5, FontFamily::Proportional),
        ),
        (egui::TextStyle::Button, FontId::new(14.5, semibold())),
        (
            egui::TextStyle::Small,
            FontId::new(12.5, FontFamily::Proportional),
        ),
        (egui::TextStyle::Monospace, FontId::new(14.5, medium())),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.interact_size.y = 30.0;
    style.visuals = visuals();
    ctx.set_style(style);
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    v.panel_fill = PAPER;
    v.window_fill = RAISED;
    v.extreme_bg_color = SUNKEN;
    v.faint_bg_color = SUNKEN;
    v.override_text_color = Some(INK);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = BRAND.gamma_multiply(0.30);
    v.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = RAISED;
    w.noninteractive.weak_bg_fill = RAISED;
    w.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, LINE);
    w.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, INK);
    w.noninteractive.corner_radius = RADIUS_S.into();

    w.inactive.bg_fill = RAISED;
    w.inactive.weak_bg_fill = RAISED;
    w.inactive.bg_stroke = egui::Stroke::new(1.5_f32, LINE);
    w.inactive.fg_stroke = egui::Stroke::new(1.0_f32, INK);
    w.inactive.corner_radius = PILL.into();

    w.hovered.bg_fill = SUNKEN;
    w.hovered.weak_bg_fill = SUNKEN;
    w.hovered.bg_stroke = egui::Stroke::new(1.5_f32, INK3);
    w.hovered.fg_stroke = egui::Stroke::new(1.0_f32, INK);
    w.hovered.corner_radius = PILL.into();
    w.hovered.expansion = 0.0;

    w.active.bg_fill = SUNKEN;
    w.active.weak_bg_fill = SUNKEN;
    w.active.bg_stroke = egui::Stroke::new(1.5_f32, INK2);
    w.active.fg_stroke = egui::Stroke::new(1.0_f32, INK);
    w.active.corner_radius = PILL.into();
    w.active.expansion = 0.0;

    w.open.bg_fill = SUNKEN;
    w.open.weak_bg_fill = SUNKEN;
    w.open.bg_stroke = egui::Stroke::new(1.5_f32, LINE);
    w.open.corner_radius = PILL.into();

    v.window_corner_radius = RADIUS_M.into();
    v.menu_corner_radius = RADIUS_M.into();
    v.window_stroke = egui::Stroke::new(1.0_f32, LINE);
    v.window_shadow = egui::Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: Color32::from_rgba_unmultiplied(0x15, 0x14, 0x2B, 72),
    };
    v.popup_shadow = v.window_shadow;
    v
}

// --- widgets -----------------------------------------------------------------

/// The one filled button on a screen: the action the screen exists for.
pub fn primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let button = egui::Button::new(RichText::new(text).color(Color32::WHITE))
        .fill(ACCENT)
        .stroke(egui::Stroke::NONE)
        .corner_radius(PILL);
    let r = ui.add(button);
    if r.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(r.rect, PILL, ACCENT_DEEP.gamma_multiply(0.35));
    }
    r
}

/// A titled section. A heading and its content; the section is separated from
/// the next by space, and only a raised panel where a panel is a real object
/// (a list of phones, a code to type in).
pub fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(title).family(display()).size(16.0).color(INK));
    ui.add_space(6.0);
    body(ui);
}

pub fn raised(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(RAISED)
        .stroke(egui::Stroke::new(1.0_f32, LINE))
        .corner_radius(RADIUS_M)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
}

pub fn hairline(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0, LINE);
}

// --- the mark ----------------------------------------------------------------

/// Path data copied verbatim from `brand/mark.svg`, 128 x 128 units. The mark
/// is stroked, not filled: two four-petal outlines with a stroke wide enough
/// that the arms fill solid. That is how it was designed, and drawing it any
/// other way changes the tips.
const PETALS: [&str; 2] = [
    "M57.0607 78.6838L40.843 94.9025C37.1883 98.5572 32.4935 100.534 27.7043 100.832C28.0025 96.0434 29.9806 91.3489 33.635 87.6945L49.8527 71.4768L57.0607 78.6838ZM94.9025 87.6945C98.5569 91.3489 100.534 96.0434 100.832 100.832C96.0434 100.534 91.3489 98.5569 87.6945 94.9025L71.4768 78.6838L78.6838 71.4768L94.9025 87.6945ZM71.4758 64.2687L64.2688 71.4758L57.0607 64.2687L64.2688 57.0607L71.4758 64.2687ZM27.7043 27.7043C32.4935 28.0022 37.1883 29.9803 40.843 33.635L57.0607 49.8527L49.8527 57.0607L33.635 40.843C29.9804 37.1883 28.0022 32.4935 27.7043 27.7043ZM100.832 27.7043C100.534 32.4935 98.5572 37.1883 94.9025 40.843L78.6838 57.0607L71.4768 49.8527L87.6945 33.635C91.3489 29.9806 96.0434 28.0025 100.832 27.7043Z",
    "M69.096 79.2898L69.0967 102.226C69.0967 107.394 67.175 112.112 63.9992 115.709C60.8238 112.112 58.9031 107.394 58.903 102.226V79.2905L69.096 79.2898ZM102.226 58.9032C107.394 58.9032 112.111 60.8246 115.708 64C112.111 67.1754 107.394 69.0968 102.226 69.0968L79.2897 69.0961V58.9039L102.226 58.9032ZM69.096 58.9039V69.0961L58.903 69.0968V58.9032L69.096 58.9039ZM12.2899 64C15.8871 60.8242 20.6055 58.9032 25.774 58.9032H48.7094V69.0968H25.774C20.6055 69.0968 15.8871 67.1758 12.2899 64ZM63.9992 12.2907C67.175 15.888 69.0967 20.6056 69.0967 25.7741L69.096 48.7102L58.903 48.7095V25.7741C58.9031 20.606 60.8238 15.8878 63.9992 12.2907Z",
];

const STROKE: f32 = 10.1935;

/// Flattens the M/L/H/V/C/Z subset the export uses into closed polylines in
/// 128-unit space. Anything else would be a change to the mark and fails here.
fn subpaths() -> Vec<Vec<egui::Pos2>> {
    let mut out = Vec::new();
    for d in PETALS {
        let mut tokens: Vec<&str> = Vec::new();
        let mut start = 0;
        let bytes = d.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b.is_ascii_alphabetic() {
                if start < i {
                    tokens.push(&d[start..i]);
                }
                tokens.push(&d[i..=i]);
                start = i + 1;
            } else if b == b' ' {
                if start < i {
                    tokens.push(&d[start..i]);
                }
                start = i + 1;
            }
        }
        if start < d.len() {
            tokens.push(&d[start..]);
        }

        let mut cur: Vec<egui::Pos2> = Vec::new();
        let (mut x, mut y) = (0.0f32, 0.0f32);
        let mut cmd = "";
        let mut i = 0;
        let num = |i: &mut usize| -> f32 {
            let v: f32 = tokens[*i].parse().expect("numeric token in mark path");
            *i += 1;
            v
        };
        while i < tokens.len() {
            let t = tokens[i];
            if t.len() == 1 && t.as_bytes()[0].is_ascii_alphabetic() {
                cmd = t;
                i += 1;
                if cmd == "Z" && !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                continue;
            }
            match cmd {
                "M" => {
                    x = num(&mut i);
                    y = num(&mut i);
                    cur.push(egui::pos2(x, y));
                    cmd = "L";
                }
                "L" => {
                    x = num(&mut i);
                    y = num(&mut i);
                    cur.push(egui::pos2(x, y));
                }
                "H" => {
                    x = num(&mut i);
                    cur.push(egui::pos2(x, y));
                }
                "V" => {
                    y = num(&mut i);
                    cur.push(egui::pos2(x, y));
                }
                "C" => {
                    let (x1, y1) = (num(&mut i), num(&mut i));
                    let (x2, y2) = (num(&mut i), num(&mut i));
                    let (x3, y3) = (num(&mut i), num(&mut i));
                    let (x0, y0) = (x, y);
                    for k in 1..=8 {
                        let t = k as f32 / 8.0;
                        let u = 1.0 - t;
                        let px = u * u * u * x0
                            + 3.0 * u * u * t * x1
                            + 3.0 * u * t * t * x2
                            + t * t * t * x3;
                        let py = u * u * u * y0
                            + 3.0 * u * u * t * y1
                            + 3.0 * u * t * t * y2
                            + t * t * t * y3;
                        cur.push(egui::pos2(px, py));
                    }
                    x = x3;
                    y = y3;
                }
                other => panic!("unsupported path command in mark: {other}"),
            }
        }
    }
    out
}

/// Draws the mark at `size` px with its top-left at `origin`.
pub fn mark(painter: &egui::Painter, origin: egui::Pos2, size: f32, color: Color32) {
    let scale = size / 128.0;
    let stroke = egui::Stroke::new(STROKE * scale, color);
    for path in subpaths() {
        let pts: Vec<egui::Pos2> = path
            .iter()
            .map(|p| egui::pos2(origin.x + p.x * scale, origin.y + p.y * scale))
            .collect();
        painter.add(egui::Shape::closed_line(pts, stroke));
    }
}

/// The window icon, from the same tile as the Android launcher icon.
pub fn icon() -> egui::IconData {
    egui::IconData {
        rgba: include_bytes!("../assets/icon-64.rgba").to_vec(),
        width: 64,
        height: 64,
    }
}
