use crate::app::{App, CenterTab};
use crate::media::{camera_key, list_cameras, video_key, VideoFrame};
use crate::model::*;
use crate::theme::{self, *};
use eframe::egui::{self, Color32, ColorImage, RichText, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::sync::Arc;

pub struct SourceTex {
    pub tex: TextureHandle,
    pub stamp: usize,
}

fn downsample(f: &VideoFrame, step: usize) -> ColorImage {
    let step = step.max(1);
    let (w, h) = (f.w as usize / step, f.h as usize / step);
    let mut px = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = y * step * f.w as usize * 4;
        for x in 0..w {
            let i = row + x * step * 4;
            px.extend_from_slice(&[f.data[i], f.data[i + 1], f.data[i + 2], 255]);
        }
    }
    ColorImage::from_rgba_unmultiplied([w, h], &px)
}

pub fn source_key(bg: &Background) -> Option<String> {
    match bg {
        Background::Camera(c) => Some(camera_key(c)),
        Background::Video(p) => Some(video_key(p, true, false)),
        _ => None,
    }
}

pub fn source_name(bg: &Background) -> String {
    match bg {
        Background::Camera(c) => camera_label(c),
        Background::Video(p) | Background::Image(p) => file_name(p),
        Background::Color(_) => "Solid colour".into(),
        Background::Gradient(_, _) => "Gradient".into(),
    }
}

impl App {
    pub fn scan_sources(&mut self) {
        match self.ffmpeg_path() {
            Some(ff) => {
                self.cameras = list_cameras(&ff);
                self.sources_scanned = true;
                if self.cameras.is_empty() {
                    self.toast("No cameras or screens found. Check the cable, or allow camera and screen recording access in system settings.", true);
                }
            }
            None => self.toast("Cameras and screen capture need FFmpeg. Open Stream setup to add it.", true),
        }
    }

    pub fn update_previews(&mut self, ctx: &egui::Context) {
        let mut wanted: HashMap<String, bool> = HashMap::new();
        if self.center_tab == CenterTab::Sources && self.shot_path.is_none() {
            let screens_ok = crate::permissions::screen_allowed() != Some(false);
            for c in &self.cameras {
                if c.screen && !screens_ok {
                    continue;
                }
                wanted.insert(camera_key(&c.spec), false);
            }
            let preview_is_blocked_screen = !screens_ok && matches!(&self.preview_src, Some(Background::Camera(c)) if is_screen_spec(c));
            if let Some(k) = self.preview_src.as_ref().and_then(source_key).filter(|_| !preview_is_blocked_screen) {
                wanted.insert(k, false);
            }
        }
        *self.sh.videos.focus.lock().unwrap() = self.preview_src.as_ref().and_then(source_key);
        *self.sh.preview.lock().unwrap() = wanted.clone();
        if wanted.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
        let preview_key = self.preview_src.as_ref().and_then(source_key);
        for k in wanted.keys() {
            let Some(f) = self.sh.videos.frame(k) else { continue };
            let stamp = Arc::as_ptr(&f) as usize;
            let big = preview_key.as_deref() == Some(k.as_str());
            let name = if big { format!("big:{k}") } else { k.clone() };
            if self.source_tex.get(&name).map(|s| s.stamp == stamp).unwrap_or(false) {
                continue;
            }
            let img = downsample(&f, if big { 2 } else { 4 });
            match self.source_tex.get_mut(&name) {
                Some(s) => {
                    s.tex.set(img, TextureOptions::LINEAR);
                    s.stamp = stamp;
                }
                None => {
                    let tex = ctx.load_texture(name.clone(), img, TextureOptions::LINEAR);
                    self.source_tex.insert(name, SourceTex { tex, stamp });
                }
            }
        }
    }

    fn image_tex(&mut self, ctx: &egui::Context, path: &str) -> Option<egui::TextureId> {
        let name = format!("img:{path}");
        if let Some(s) = self.source_tex.get(&name) {
            return Some(s.tex.id());
        }
        let img = match self.sh.images.get(path) {
            Some(Ok(img)) => img,
            _ => return None,
        };
        let small = image::imageops::thumbnail(&*img, 640, 360);
        let ci = ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw());
        let tex = ctx.load_texture(name.clone(), ci, TextureOptions::LINEAR);
        let id = tex.id();
        self.source_tex.insert(name, SourceTex { tex, stamp: 0 });
        Some(id)
    }

    fn texture_for(&mut self, ctx: &egui::Context, bg: &Background, big: bool) -> Option<egui::TextureId> {
        match bg {
            Background::Image(p) => self.image_tex(ctx, p),
            _ => {
                let k = source_key(bg)?;
                let name = if big { format!("big:{k}") } else { k };
                self.source_tex.get(&name).or_else(|| self.source_tex.get(&name.trim_start_matches("big:").to_string())).map(|s| s.tex.id())
            }
        }
    }

    fn on_air(&self, bg: &Background) -> (bool, bool) {
        let st = self.sh.snapshot();
        let uses = |o: &OutputLive| o.background == *bg || o.pip.as_ref() == Some(bg);
        if st.mirror {
            let u = uses(&st.outputs[0]);
            (u, u)
        } else {
            (uses(&st.outputs[0]), uses(&st.outputs[1]))
        }
    }

    fn paint_source(&mut self, ui: &mut egui::Ui, bg: &Background, size: Vec2, big: bool, selected: bool) -> egui::Response {
        let ctx = ui.ctx().clone();
        let tex = self.texture_for(&ctx, bg, big);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let p = ui.painter();
        p.rect_filled(rect, 6, Color32::BLACK);
        match (tex, bg) {
            (Some(id), _) => {
                p.image(id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                if let Some(k) = source_key(bg) {
                    if !self.sh.videos.is_live_capture(&k) {
                        let band = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 22.0), rect.right_bottom());
                        p.rect_filled(band, 0, Color32::from_black_alpha(170));
                        p.text(band.center(), egui::Align2::CENTER_CENTER, "Still picture · click to view live", egui::FontId::new(11.0, egui::FontFamily::Proportional), TEXT);
                    }
                }
            }
            (None, Background::Color(c)) => {
                p.rect_filled(rect, 6, rgb(*c));
            }
            (None, Background::Gradient(a, _)) => {
                p.rect_filled(rect, 6, rgb(*a));
            }
            (None, _) => {
                let key = source_key(bg).unwrap_or_default();
                let err = self.sh.videos.error_for(&key);
                let wait = self.sh.videos.waiting_for(&key).unwrap_or(0.0);
                let blocked = matches!(bg, Background::Camera(c) if is_screen_spec(c)) && crate::permissions::screen_allowed() == Some(false);
                let paused = !blocked && !self.sh.videos.is_live_capture(&key);
                let (msg, c) = match (err, wait) {
                    _ if blocked => ("Needs screen permission", GOLD),
                    _ if paused => ("Click to view live", MUTED),
                    (Some(_), _) => ("Can't open: see help", RED),
                    (None, w) if w > 6.0 => ("No picture yet: allow access?", GOLD),
                    _ => ("Starting…", MUTED),
                };
                p.text(rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::new(12.0, egui::FontFamily::Proportional), c);
            }
        }
        let (scr, stm) = self.on_air(bg);
        if scr || stm {
            let label = match (scr, stm) {
                (true, true) => "ON AIR",
                (true, false) => "ON SCREEN",
                _ => "ON STREAM",
            };
            let r = egui::Rect::from_min_size(rect.left_top() + Vec2::new(6.0, 6.0), Vec2::new(if big { 90.0 } else { 74.0 }, 18.0));
            p.rect_filled(r, 4, RED);
            p.text(r.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::new(10.5, bold_family()), Color32::WHITE);
        }
        let stroke = if selected { Stroke::new(2.0, GOLD) } else if resp.hovered() { Stroke::new(1.0, GOLD_DIM) } else { Stroke::new(1.0, LINE) };
        p.rect_stroke(rect, 6, stroke, egui::StrokeKind::Outside);
        resp
    }

    pub fn sources_panel(&mut self, ui: &mut egui::Ui) {
        if !self.sources_scanned && self.ff.is_some() && self.shot_path.is_none() {
            self.scan_sources();
        }
        let avail = ui.available_width();
        let left_w = (avail * 0.56).max(360.0);
        let right_w = avail - left_w - 12.0;
        egui::ScrollArea::vertical().id_salt("sources").auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(left_w);
                    self.focused_preview(ui, left_w);
                });
                ui.vertical(|ui| {
                    ui.set_width(right_w);
                    self.source_tiles(ui, right_w);
                });
            });
        });
    }

    fn focused_preview(&mut self, ui: &mut egui::Ui, width: f32) {
        theme::card_frame().show(ui, |ui| {
            ui.set_width(width - 22.0);
            let name = self.preview_src.as_ref().map(source_name).unwrap_or_else(|| "Pick a camera or screen on the right".into());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Preview").family(heading_family()).size(16.0));
                ui.label(RichText::new(&name).size(12.5).color(MUTED));
            });
            hint(ui, "Only you see this preview. Nothing changes on the screen or stream until you press a button below.");
            let w = ui.available_width();
            let size = Vec2::new(w, w * 9.0 / 16.0);
            match self.preview_src.clone() {
                Some(bg) => {
                    self.paint_source(ui, &bg, size, true, false);
                }
                None => {
                    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                    ui.painter().rect_filled(rect, 6, Color32::BLACK);
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "No source selected", egui::FontId::new(14.0, egui::FontFamily::Proportional), MUTED);
                }
            }
            let Some(bg) = self.preview_src.clone() else { return };
            if let Some(k) = source_key(&bg) {
                if self.sh.videos.screen_on_air_elsewhere(&k) {
                    hint(ui, "Another screen is on air right now. A Mac can capture one screen at a time, so this one shows a still picture until that screen comes off air.");
                } else if crate::media::VideoHub::single_screen_rule() && crate::media::is_screen_key(&k) {
                    hint(ui, "A Mac captures one screen at a time. The screen you preview or put on air is live; the other screen tiles show their last picture.");
                }
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if big_button(ui, "Show full picture", GOLD, INK, Vec2::new(0.0, 32.0)).on_hover_text("Put this source behind everything, filling the picture").clicked() {
                    self.send_background(bg.clone());
                    let target = self.send_target;
                    self.sh.edit(|s| {
                        for o in [OutputId::Screen, OutputId::Stream] {
                            if s.mirror || target.includes(o) {
                                if s.out(o).pip.as_ref() == Some(&bg) {
                                    s.out_mut(o).set_pip(None, PipLayout::Off);
                                }
                            }
                        }
                    });
                }
                if big_button(ui, "Corner picture", CARD_HI, TEXT, Vec2::new(0.0, 32.0)).on_hover_text("Show this source small in the top-right corner over the main picture").clicked() {
                    self.send_pip(Some(bg.clone()), PipLayout::Corner);
                }
                if big_button(ui, "Next to slides", CARD_HI, TEXT, Vec2::new(0.0, 32.0)).on_hover_text("Slide text on one half, this camera or screen on the other half").clicked() {
                    self.send_background(bg.clone());
                    self.set_layout(TextLayout::SideLeft);
                }
                if big_button(ui, "Two pictures side by side", CARD_HI, TEXT, Vec2::new(0.0, 32.0)).on_hover_text("Split the picture: the main picture on the left, this source on the right").clicked() {
                    self.send_pip(Some(bg.clone()), PipLayout::Side);
                }
            });
            ui.horizontal_wrapped(|ui| {
                let st = self.sh.snapshot();
                let o = if st.mirror || self.send_target != SendTarget::Stream { &st.outputs[0] } else { &st.outputs[1] };
                let (size, corner) = (o.pip_size, o.pip_corner);
                ui.label(RichText::new("Corner picture size").size(12.0).color(MUTED));
                let mut new_size = size;
                for (label, v) in [("Small", 0.26), ("Medium", 0.36), ("Large", 0.48)] {
                    if chip(ui, (size - v).abs() < 0.01, label).clicked() {
                        new_size = v;
                    }
                }
                ui.label(RichText::new("Place").size(12.0).color(MUTED));
                let mut new_corner = corner;
                for (label, v, tip) in [("↖", 0u8, "Top left"), ("↗", 1, "Top right"), ("↙", 2, "Bottom left"), ("↘", 3, "Bottom right")] {
                    if chip(ui, corner == v, label).on_hover_text(tip).clicked() {
                        new_corner = v;
                    }
                }
                if new_size != size || new_corner != corner {
                    let target = self.send_target;
                    self.sh.edit(|s| {
                        for out in [OutputId::Screen, OutputId::Stream] {
                            if s.mirror || target.includes(out) {
                                let ol = s.out_mut(out);
                                ol.pip_size = new_size;
                                ol.pip_corner = new_corner;
                            }
                        }
                    });
                }
            });
            if let Some(k) = source_key(&bg) {
                let err = self.sh.videos.error_for(&k);
                let waiting = self.sh.videos.waiting_for(&k).unwrap_or(0.0);
                if err.is_some() || waiting > 6.0 {
                    let is_screen = matches!(&bg, Background::Camera(c) if is_screen_spec(c));
                    egui::Frame::new().fill(egui::Color32::from_rgb(58, 46, 26)).corner_radius(8).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(if is_screen { "The screen picture isn't coming through" } else { "The camera picture isn't coming through" }).family(semi_family()).color(GOLD));
                        let how = if cfg!(target_os = "macos") {
                            if is_screen {
                                "Mac: open System Settings, Privacy & Security, Screen & System Audio Recording, and switch on Lampstand (or the app you started it from). Then press Try again."
                            } else {
                                "Mac: open System Settings, Privacy & Security, Camera, and switch on Lampstand (or the app you started it from). Close other apps using the camera, then press Try again."
                            }
                        } else if cfg!(windows) {
                            "Windows: open Settings, Privacy & security, Camera, and allow desktop apps. Close other apps using the camera (Zoom, Teams, OBS), then press Try again."
                        } else {
                            "Linux: check that your user can open the camera (video group) and that you're in an X11 session for screen capture."
                        };
                        ui.label(RichText::new(how).size(12.5));
                        if let Some(e) = err {
                            ui.label(RichText::new(format!("Details: {}", e.chars().take(140).collect::<String>())).size(11.5).color(MUTED));
                        }
                        ui.horizontal(|ui| {
                            if small_button(ui, "Try again").clicked() {
                                self.sh.videos.restart(&k);
                            }
                            if !cfg!(target_os = "linux") && small_button(ui, "Open privacy settings").clicked() {
                                open_privacy_settings(is_screen);
                            }
                        });
                    });
                }
            }
            if let Background::Camera(spec) = &bg {
                ui.add_space(6.0);
                subheading(ui, "Camera framing (digital zoom)");
                let mut adj = self.sh.snapshot().cam_adjust.get(spec).copied().unwrap_or_default();
                let before = adj;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Zoom").size(12.0).color(MUTED));
                    ui.add(egui::Slider::new(&mut adj.zoom, 1.0..=4.0).max_decimals(1).suffix("×"));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Left / right").size(12.0).color(MUTED));
                    ui.add(egui::Slider::new(&mut adj.x, -1.0..=1.0).show_value(false));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Up / down").size(12.0).color(MUTED));
                    ui.add(egui::Slider::new(&mut adj.y, -1.0..=1.0).show_value(false));
                });
                ui.horizontal(|ui| {
                    for (label, z, x, y) in [("Wide", 1.0, 0.0, 0.0), ("Pulpit", 2.0, 0.0, -0.2), ("Close-up", 3.0, 0.0, -0.3)] {
                        if small_button(ui, label).clicked() {
                            adj = CamAdjust { zoom: z, x, y };
                        }
                    }
                });
                if adj != before {
                    let spec = spec.clone();
                    self.sh.edit(|s| {
                        s.cam_adjust.insert(spec, adj);
                    });
                }
                hint(ui, "Framing applies wherever this camera is shown. For motorised cameras, keep using the camera's own remote.");
            }
        });
    }

    fn source_tiles(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.horizontal(|ui| {
            heading(ui, "Sources");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if small_button(ui, "Find cameras & screens").clicked() {
                    self.scan_sources();
                }
            });
        });
        let cols = 2usize;
        let tw = (width - 10.0) / cols as f32;
        let th = tw * 9.0 / 16.0;
        let mut pick: Option<Background> = None;
        for (screens, title, empty) in [
            (false, "Cameras & capture cards", "No cameras found yet. Plug one in and press Find."),
            (true, "Screen capture", "Shows a computer screen, for example slides from a laptop or a website."),
        ] {
            ui.add_space(4.0);
            subheading(ui, title);
            if screens {
                self.screen_permission_card(ui);
                ui.horizontal_wrapped(|ui| {
                    if big_button(ui, "Choose screen or window…", GOLD, INK, Vec2::new(0.0, 30.0)).on_hover_text("Pick a whole display or a single app window to show").clicked() {
                        self.open_picker();
                    }
                    let mut cursor = crate::capture::SHOW_CURSOR.load(std::sync::atomic::Ordering::Relaxed);
                    if ui.checkbox(&mut cursor, "Show mouse pointer").on_hover_text("Applies the next time a screen starts; press Try again to apply now").changed() {
                        crate::capture::SHOW_CURSOR.store(cursor, std::sync::atomic::Ordering::Relaxed);
                    }
                });
            }
            let list: Vec<_> = self.cameras.iter().filter(|c| c.screen == screens).cloned().collect();
            if list.is_empty() {
                hint(ui, empty);
                continue;
            }
            for row in list.chunks(cols) {
                ui.horizontal(|ui| {
                    for c in row {
                        ui.vertical(|ui| {
                            ui.set_width(tw);
                            let bg = Background::Camera(c.spec.clone());
                            let sel = self.preview_src.as_ref() == Some(&bg);
                            if self.paint_source(ui, &bg, Vec2::new(tw, th), false, sel).clicked() {
                                pick = Some(bg);
                            }
                            ui.label(RichText::new(&c.name).size(12.0));
                        });
                    }
                });
            }
        }
        let media: Vec<MediaFile> = self.lib.media.iter().filter(|m| m.kind != MediaKind::Audio).cloned().collect();
        ui.add_space(4.0);
        subheading(ui, "Pictures & videos");
        if media.is_empty() {
            hint(ui, "Add pictures and videos in the Media tab on the left.");
        }
        for row in media.chunks(cols) {
            ui.horizontal(|ui| {
                for m in row {
                    ui.vertical(|ui| {
                        ui.set_width(tw);
                        let bg = if m.kind == MediaKind::Image { Background::Image(m.path.clone()) } else { Background::Video(m.path.clone()) };
                        let sel = self.preview_src.as_ref() == Some(&bg);
                        if self.paint_source(ui, &bg, Vec2::new(tw, th), false, sel).clicked() {
                            pick = Some(bg);
                        }
                        ui.label(RichText::new(file_name(&m.path)).size(12.0));
                    });
                }
            });
        }
        if let Some(bg) = pick {
            self.preview_src = Some(bg);
        }
        ui.add_space(8.0);
        subheading(ui, "On air now");
        let st = self.sh.snapshot();
        let outs: Vec<(OutputId, &str)> = if st.mirror { vec![(OutputId::Screen, "Screen & stream")] } else { vec![(OutputId::Screen, "Church screen"), (OutputId::Stream, "Stream")] };
        for (o, label) in outs {
            let ol = st.out(o);
            let extra = match (&ol.pip, ol.pip_layout) {
                (Some(p), PipLayout::Corner) => format!(" + {} in the corner", source_name(p)),
                (Some(p), PipLayout::Side) => format!(" | {} side by side", source_name(p)),
                _ => String::new(),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{label}:")).size(12.5).color(MUTED));
                ui.label(RichText::new(format!("{}{}", source_name(&ol.background), extra)).size(12.5));
                if ol.pip.is_some() && small_button(ui, "Back to one picture").clicked() {
                    let mirror = st.mirror;
                    self.sh.edit(|s| {
                        if mirror {
                            s.send_pip(SendTarget::Both, None, PipLayout::Off);
                        } else {
                            s.out_mut(o).set_pip(None, PipLayout::Off);
                        }
                    });
                }
            });
        }
    }

    pub fn send_pip(&mut self, pip: Option<Background>, layout: PipLayout) {
        let target = self.send_target;
        self.sh.edit(|s| s.send_pip(target, pip, layout));
    }

    pub fn toggle_clean(&mut self, only: Option<OutputId>) {
        let target = self.send_target;
        self.sh.edit(|s| {
            let mirror = s.mirror;
            for o in [OutputId::Screen, OutputId::Stream] {
                let applies = match only {
                    Some(x) => mirror || x == o,
                    None => mirror || target.includes(o),
                };
                if applies {
                    let c = !s.out(o).clean;
                    s.out_mut(o).set_clean(c);
                }
            }
            s.touch();
        });
    }
}

fn open_privacy_settings(screen: bool) {
    if cfg!(target_os = "macos") {
        let pane = if screen { "Privacy_ScreenCapture" } else { "Privacy_Camera" };
        let _ = std::process::Command::new("open").arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}")).spawn();
    } else if cfg!(windows) {
        let page = if screen { "ms-settings:privacy" } else { "ms-settings:privacy-webcam" };
        let _ = std::process::Command::new("cmd").args(["/C", "start", "", page]).spawn();
    }
}

impl App {
    pub fn set_layout(&mut self, layout: TextLayout) {
        let target = self.send_target;
        self.sh.edit(|s| {
            if s.mirror {
                s.outputs[0].look.layout = layout;
                s.outputs[1].look.layout = layout;
            } else {
                for o in [OutputId::Screen, OutputId::Stream] {
                    if target.includes(o) {
                        s.out_mut(o).look.layout = layout;
                    }
                }
            }
        });
    }

    pub fn current_layout(&self) -> TextLayout {
        let st = self.sh.snapshot();
        if st.mirror || self.send_target != SendTarget::Stream { st.outputs[0].look.layout } else { st.outputs[1].look.layout }
    }

    pub fn open_picker(&mut self) {
        let Some(ff) = self.ffmpeg_path() else {
            self.toast("Screen capture needs FFmpeg. Open Stream setup to add it.", true);
            return;
        };
        let (items, note) = crate::capture::list_capture_targets(&ff);
        self.picker_items = items;
        self.picker_note = note;
        self.picker_filter.clear();
        self.show_picker = true;
    }

    pub fn capture_picker(&mut self, ctx: &egui::Context) {
        if !self.show_picker {
            return;
        }
        let mut open = true;
        let mut chosen: Option<crate::capture::CaptureTarget> = None;
        let mut refresh = false;
        egui::Window::new(RichText::new("Choose what to capture").family(heading_family()).size(19.0))
            .open(&mut open)
            .collapsible(false)
            .default_width(520.0)
            .default_height(520.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.picker_filter).hint_text("Search by app or window name").desired_width(ui.available_width() - 80.0));
                    if small_button(ui, "Refresh").clicked() {
                        refresh = true;
                    }
                });
                if !self.picker_note.is_empty() {
                    hint(ui, &self.picker_note);
                }
                let filter = self.picker_filter.to_lowercase();
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (window, title) in [(false, "Whole screens"), (true, "Single windows")] {
                        ui.add_space(4.0);
                        subheading(ui, title);
                        let items: Vec<_> = self.picker_items.iter().filter(|t| t.window == window && (filter.is_empty() || t.name.to_lowercase().contains(&filter))).cloned().collect();
                        if items.is_empty() {
                            hint(ui, if window { "No windows found." } else { "No screens found." });
                        }
                        for t in items {
                            let r = theme::card_frame()
                                .inner_margin(egui::Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(RichText::new(format!("{} {}", if window { "🗔" } else { "🖥" }, t.name)).size(13.5));
                                    ui.label(RichText::new(&t.detail).size(11.5).color(MUTED));
                                })
                                .response
                                .interact(Sense::click());
                            if r.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if r.clicked() {
                                chosen = Some(t.clone());
                            }
                        }
                    }
                });
            });
        if refresh {
            self.open_picker();
        }
        if let Some(t) = chosen {
            let dev = crate::media::CameraDevice { spec: t.spec.clone(), name: t.name.clone(), screen: true };
            if !self.cameras.iter().any(|c| c.spec == dev.spec) {
                self.cameras.push(dev);
            }
            self.preview_src = Some(Background::Camera(t.spec));
            self.center_tab = crate::app::CenterTab::Sources;
            self.show_picker = false;
            return;
        }
        if !open {
            self.show_picker = false;
        }
    }
}

pub fn is_screen_spec(spec: &str) -> bool {
    spec.starts_with("screen:") || spec.starts_with("window:")
}

impl App {
    pub fn screen_permission_card(&mut self, ui: &mut egui::Ui) {
        if crate::permissions::screen_allowed() != Some(false) {
            return;
        }
        egui::Frame::new().fill(egui::Color32::from_rgb(58, 46, 26)).corner_radius(8).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("macOS isn't letting this copy of Lampstand record the screen yet").family(semi_family()).color(GOLD));
            ui.label(
                RichText::new(
                    "If Lampstand is already switched on in System Settings, that switch belongs to an older copy (each update looks new to macOS). Fix it once with these three steps:",
                )
                .size(12.5),
            );
            ui.horizontal_wrapped(|ui| {
                if small_button(ui, "1. Reset permission").on_hover_text("Removes the old Lampstand entry from Screen Recording").clicked() {
                    match crate::permissions::reset_screen_permission() {
                        Ok(()) => self.toast("Old screen permission removed. Now press 2. Ask for access.", false),
                        Err(e) => self.toast(
                            format!("Couldn't reset it automatically ({e}). In System Settings, select Lampstand under Screen & System Audio Recording and click the minus button."),
                            true,
                        ),
                    }
                }
                if small_button(ui, "2. Ask for access").on_hover_text("macOS shows its permission window; choose Allow or Open System Settings and switch Lampstand on").clicked() {
                    crate::permissions::ask_screen();
                    self.screen_asked = true;
                }
                let restart = egui::Button::new(RichText::new("3. Restart Lampstand").size(13.0).color(if self.screen_asked { INK } else { TEXT })).fill(if self.screen_asked { GOLD } else { BTN });
                if ui.add(restart).on_hover_text("macOS only applies screen permission after the app restarts").clicked() {
                    self.persist();
                    if crate::permissions::relaunch() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    } else {
                        self.toast("Please quit Lampstand and open it again.", true);
                    }
                }
                if small_button(ui, "Open privacy settings").clicked() {
                    open_privacy_settings(true);
                }
            });
        });
    }
}
