use crate::text::{SANS, SANS_BOLD, SANS_SEMI, SERIF};
use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText, Stroke, Vec2};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(17, 19, 25);
pub const PANEL: Color32 = Color32::from_rgb(24, 27, 35);
pub const CARD: Color32 = Color32::from_rgb(33, 37, 48);
pub const CARD_HI: Color32 = Color32::from_rgb(46, 51, 66);
pub const BTN: Color32 = Color32::from_rgb(45, 50, 64);
pub const BTN_HOVER: Color32 = Color32::from_rgb(60, 66, 84);
pub const LINE: Color32 = Color32::from_rgb(52, 57, 72);
pub const TEXT: Color32 = Color32::from_rgb(232, 233, 238);
pub const MUTED: Color32 = Color32::from_rgb(150, 155, 172);
pub const GOLD: Color32 = Color32::from_rgb(224, 178, 82);
pub const GOLD_DIM: Color32 = Color32::from_rgb(120, 96, 48);
pub const RED: Color32 = Color32::from_rgb(229, 72, 77);
pub const GREEN: Color32 = Color32::from_rgb(61, 214, 140);
pub const INK: Color32 = Color32::from_rgb(16, 18, 28);

pub fn heading_family() -> FontFamily {
    FontFamily::Name("heading".into())
}

pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn semi_family() -> FontFamily {
    FontFamily::Name("semi".into())
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("figtree".into(), Arc::new(FontData::from_static(SANS)));
    fonts.font_data.insert("figtree-semi".into(), Arc::new(FontData::from_static(SANS_SEMI)));
    fonts.font_data.insert("figtree-bold".into(), Arc::new(FontData::from_static(SANS_BOLD)));
    fonts.font_data.insert("playfair".into(), Arc::new(FontData::from_static(SERIF)));
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut prop = vec!["figtree".to_string()];
    prop.extend(fallback.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, prop);
    for (fam, name) in [(heading_family(), "playfair"), (bold_family(), "figtree-bold"), (semi_family(), "figtree-semi")] {
        let mut v = vec![name.to_string()];
        v.extend(fallback.iter().cloned());
        fonts.families.insert(fam, v);
    }
    ctx.set_fonts(fonts);

    let mut style = (*ctx.global_style()).clone();
    style.text_styles.insert(egui::TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Button, FontId::new(14.0, FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Small, FontId::new(12.0, FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Heading, FontId::new(22.0, heading_family()));
    style.spacing.item_spacing = Vec2::new(8.0, 7.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.interact_size.y = 28.0;
    style.spacing.slider_width = 170.0;
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(12, 13, 18);
    v.faint_bg_color = CARD;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(12);
    v.selection.bg_fill = GOLD_DIM;
    v.selection.stroke = Stroke::new(1.0, GOLD);
    v.hyperlink_color = GOLD;
    v.widgets.noninteractive.bg_fill = PANEL;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.inactive.bg_fill = BTN;
    v.widgets.inactive.weak_bg_fill = BTN;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.corner_radius = CornerRadius::same(7);
    v.widgets.hovered.bg_fill = BTN_HOVER;
    v.widgets.hovered.weak_bg_fill = BTN_HOVER;
    v.widgets.hovered.corner_radius = CornerRadius::same(7);
    v.widgets.active.bg_fill = BTN_HOVER;
    v.widgets.active.weak_bg_fill = BTN_HOVER;
    v.widgets.active.corner_radius = CornerRadius::same(7);
    v.widgets.open.corner_radius = CornerRadius::same(7);
    ctx.set_global_style(style);
}

pub fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).family(heading_family()).size(19.0).color(TEXT));
}

pub fn subheading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).family(semi_family()).size(11.5).color(MUTED));
}

pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.5).color(MUTED));
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::new().fill(CARD).corner_radius(10).inner_margin(Margin::same(10)).stroke(Stroke::new(1.0, LINE))
}

pub fn big_button(ui: &mut egui::Ui, text: &str, fill: Color32, fg: Color32, min: Vec2) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).family(bold_family()).size(14.5).color(fg)).fill(fill).min_size(min).corner_radius(8))
}

pub fn small_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).size(13.0)).min_size(Vec2::new(0.0, 26.0)))
}

pub fn chip(ui: &mut egui::Ui, on: bool, text: &str) -> egui::Response {
    let (fill, fg) = if on { (GOLD, INK) } else { (BTN, Color32::from_rgb(190, 194, 208)) };
    ui.add(egui::Button::new(RichText::new(text).family(semi_family()).size(12.5).color(fg)).fill(fill).corner_radius(14).min_size(Vec2::new(0.0, 24.0)))
}

pub fn meter(ui: &mut egui::Ui, level: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 8.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4, Color32::from_rgb(10, 11, 15));
    let db = if level > 0.0001 { 20.0 * level.log10() } else { -60.0 };
    let frac = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
    let mut r = rect;
    r.set_width(rect.width() * frac);
    let c = if db > -3.0 { RED } else if db > -12.0 { GOLD } else { GREEN };
    p.rect_filled(r, 4, c);
}

pub fn status_dot(ui: &mut egui::Ui, c: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, c);
}

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}
