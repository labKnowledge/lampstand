use crate::app::App;
use crate::model::*;
use crate::theme::*;
use eframe::egui::{self, Color32, Key, RichText, Sense, Vec2};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WinMode {
    Window,
    Mini,
    Full(usize),
}

pub fn out_name(out: OutputId) -> &'static str {
    match out {
        OutputId::Screen => "In-church screen",
        OutputId::Stream => "Live stream",
    }
}

pub fn describe(mode: Option<WinMode>, displays: &[String]) -> String {
    match mode {
        None => "No output window".into(),
        Some(WinMode::Window) => "Open in a window".into(),
        Some(WinMode::Mini) => "Mini window on top".into(),
        Some(WinMode::Full(i)) => {
            let d = displays.get(i).cloned().unwrap_or_else(|| format!("Display {}", i + 1));
            format!("Full screen on {}", d.split(" · ").next().unwrap_or(&d).split(" (").next().unwrap_or(&d))
        }
    }
}

fn viewport_id(out: OutputId, mode: WinMode) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("lampstand-output", out.index(), format!("{mode:?}")))
}

enum MenuAction {
    Win(Option<WinMode>),
    ClearText,
    Clean,
    Black,
    Snapshot,
    Record,
    Live,
    RefreshDisplays,
}

fn window_items(ui: &mut egui::Ui, current: Option<WinMode>, displays: &[String], act: &mut Option<MenuAction>) {
    ui.menu_button("🖥  Show full screen on", |ui| {
        for (i, d) in displays.iter().enumerate() {
            let on = current == Some(WinMode::Full(i));
            if ui.add(egui::Button::selectable(on, d.as_str())).clicked() {
                *act = Some(MenuAction::Win(Some(WinMode::Full(i))));
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Look for displays again").clicked() {
            *act = Some(MenuAction::RefreshDisplays);
            ui.close();
        }
    });
    if ui.add(egui::Button::selectable(current == Some(WinMode::Window), "🗔  Open in a window")).clicked() {
        *act = Some(MenuAction::Win(Some(WinMode::Window)));
        ui.close();
    }
    if ui
        .add(egui::Button::selectable(current == Some(WinMode::Mini), "▣  Mini window (stays on top)"))
        .on_hover_text("A small floating copy you can keep in a corner of your screen")
        .clicked()
    {
        *act = Some(MenuAction::Win(Some(WinMode::Mini)));
        ui.close();
    }
    if current.is_some() && ui.button("✕  Close output window").clicked() {
        *act = Some(MenuAction::Win(None));
        ui.close();
    }
}

impl App {
    pub fn displays(&self) -> Vec<String> {
        self.display_list.lock().unwrap().iter().map(|d| d.name.clone()).collect()
    }

    pub fn display_rect(&self, i: usize) -> Option<egui::Rect> {
        let r = self.display_list.lock().unwrap().get(i)?.rect?;
        let scale = if cfg!(target_os = "macos") { 1.0 } else { self.pixels_per_point.max(0.5) };
        Some(egui::Rect::from_min_size(egui::pos2(r[0], r[1]) / scale, Vec2::new(r[2], r[3]) / scale))
    }

    fn app_display(&self) -> Option<usize> {
        let Some(r) = self.main_rect else { return Some(0) };
        let c = r.center();
        let n = self.display_list.lock().unwrap().len();
        (0..n).find(|i| self.display_rect(*i).map(|r| r.contains(c)).unwrap_or(false))
    }

    pub fn refresh_displays(&self) {
        let slot = self.display_list.clone();
        std::thread::spawn(move || {
            let d = crate::capture::list_displays();
            *slot.lock().unwrap() = d;
        });
    }

    pub fn set_out_win(&mut self, out: OutputId, mode: Option<WinMode>) {
        let mode = match mode {
            Some(WinMode::Full(i)) if self.app_display() == Some(i) => {
                self.toast("That is the display Lampstand is on, so the output opens in a window instead and the app stays usable. Move Lampstand to another display, or pick the projector display.", true);
                Some(WinMode::Window)
            }
            m => m,
        };
        self.out_win[out.index()] = mode;
    }

    pub fn toggle_projector(&mut self) {
        if self.out_win[0].is_some() {
            self.out_win[0] = None;
            return;
        }
        let n = self.display_list.lock().unwrap().len();
        let wanted = self.settings.projector_display.saturating_sub(1);
        let app_on = self.app_display();
        let pick = if app_on != Some(wanted) && wanted < n {
            Some(wanted)
        } else {
            (0..n).find(|i| Some(*i) != app_on && self.display_rect(*i).is_some())
        };
        match pick {
            Some(i) => self.set_out_win(OutputId::Screen, Some(WinMode::Full(i))),
            None => {
                self.toast("Only one display found, so the projector opens in a window. Connect the projector or TV and choose it from the right-click menu.", false);
                self.set_out_win(OutputId::Screen, Some(WinMode::Window));
            }
        }
    }

    pub fn output_menu(&mut self, ui: &mut egui::Ui, out: OutputId) {
        let st = self.sh.snapshot();
        let o = if st.mirror { st.outputs[0].clone() } else { st.out(out).clone() };
        let displays = self.displays();
        let mut act: Option<MenuAction> = None;
        ui.set_min_width(250.0);
        ui.label(RichText::new(out_name(out)).family(heading_family()).size(15.0));
        ui.label(RichText::new(describe(self.out_win[out.index()], &displays)).size(11.5).color(MUTED));
        ui.separator();
        window_items(ui, self.out_win[out.index()], &displays, &mut act);
        ui.separator();
        if ui.button("Clear text").clicked() {
            act = Some(MenuAction::ClearText);
            ui.close();
        }
        if ui.button(if o.clean { "Show text & graphics" } else { "Clear view (hide text & graphics)" }).clicked() {
            act = Some(MenuAction::Clean);
            ui.close();
        }
        if ui.button(if o.black { "Unblack" } else { "Fade to black" }).clicked() {
            act = Some(MenuAction::Black);
            ui.close();
        }
        if ui.button("📷  Save a picture of this output").clicked() {
            act = Some(MenuAction::Snapshot);
            ui.close();
        }
        if out == OutputId::Stream {
            ui.separator();
            let rec = if self.rec.is_some() { "■  Stop recording" } else { "●  Start recording" };
            if ui.button(rec).clicked() {
                act = Some(MenuAction::Record);
                ui.close();
            }
            let live = if self.live.is_some() { "End live stream…" } else { "Go live" };
            if ui.button(live).clicked() {
                act = Some(MenuAction::Live);
                ui.close();
            }
        }
        if st.mirror {
            ui.separator();
            hint(ui, "Same picture mode: text and black apply to both outputs.");
        }
        if let Some(a) = act {
            self.run_menu_action(out, a);
        }
    }

    fn run_menu_action(&mut self, out: OutputId, a: MenuAction) {
        match a {
            MenuAction::Win(m) => self.set_out_win(out, m),
            MenuAction::RefreshDisplays => self.refresh_displays(),
            MenuAction::ClearText => {
                let tr = self.transition;
                self.sh.edit(|s| {
                    if s.mirror {
                        s.send_slide(SendTarget::Both, None, tr);
                    } else {
                        s.out_mut(out).set_slide(None, tr);
                    }
                });
                self.strip.live = None;
            }
            MenuAction::Clean => self.toggle_clean(Some(out)),
            MenuAction::Black => self.toggle_black(Some(out)),
            MenuAction::Snapshot => self.snapshot(out),
            MenuAction::Record => {
                if self.rec.is_some() {
                    self.stop_recording();
                } else {
                    self.start_recording();
                }
            }
            MenuAction::Live => {
                if self.live.is_some() {
                    self.confirm_end = true;
                } else {
                    self.go_live();
                }
            }
        }
    }

    pub fn snapshot(&mut self, out: OutputId) {
        let (w, h, data) = {
            let f = self.sh.frames.lock().unwrap();
            (f.w, f.h, if out == OutputId::Screen { f.screen.clone() } else { f.stream.clone() })
        };
        let dir = PathBuf::from(&self.settings.record_dir);
        let _ = std::fs::create_dir_all(&dir);
        let which = if out == OutputId::Screen { "Screen" } else { "Stream" };
        let path = dir.join(format!("{which} snapshot {}.png", chrono::Local::now().format("%Y-%m-%d %H-%M-%S")));
        match image::RgbaImage::from_raw(w, h, data.to_vec()).map(|img| img.save(&path)) {
            Some(Ok(())) => self.toast(format!("Saved {}", path.display()), false),
            _ => self.toast("Couldn't save the picture", true),
        }
    }

    pub fn output_windows(&mut self, ctx: &egui::Context) {
        let displays = self.displays();
        for out in [OutputId::Screen, OutputId::Stream] {
            let i = out.index();
            let Some(mode) = self.out_win[i] else { continue };
            let id = viewport_id(out, mode);
            let title = format!("Lampstand · {}", out_name(out));
            let mut builder = egui::ViewportBuilder::default().with_title(title).with_active(false);
            builder = match mode {
                WinMode::Full(d) => match self.display_rect(d) {
                    Some(r) => builder
                        .with_decorations(false)
                        .with_position(r.min + Vec2::splat(1.0))
                        .with_inner_size(r.size())
                        .with_resizable(false)
                        .with_fullscreen(!cfg!(target_os = "macos"))
                        .with_window_level(egui::WindowLevel::AlwaysOnTop),
                    None => builder.with_monitor(d).with_fullscreen(true),
                },
                WinMode::Window => builder.with_inner_size([960.0, 540.0]),
                WinMode::Mini => builder.with_inner_size([400.0, 225.0]).with_window_level(egui::WindowLevel::AlwaysOnTop),
            };
            let tex = self.tex[i].as_ref().map(|t| t.id());
            let projector_display = self.settings.projector_display.saturating_sub(1);
            let mut action: Option<Option<WinMode>> = None;
            let mut menu_act: Option<MenuAction> = None;
            ctx.show_viewport_immediate(id, builder, |ui, _class| {
                let full = ui.available_rect_before_wrap();
                let resp = ui.allocate_rect(full, Sense::click());
                let p = ui.painter();
                p.rect_filled(full, 0, Color32::BLACK);
                if let Some(tid) = tex {
                    let w = full.width().min(full.height() * 16.0 / 9.0);
                    let h = w * 9.0 / 16.0;
                    let r = egui::Rect::from_center_size(full.center(), Vec2::new(w, h));
                    p.image(tid, r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                }
                if resp.double_clicked() {
                    action = Some(Some(if matches!(mode, WinMode::Full(_)) { WinMode::Window } else { WinMode::Full(projector_display) }));
                }
                resp.context_menu(|ui| {
                    ui.label(RichText::new(out_name(out)).family(heading_family()).size(15.0));
                    ui.separator();
                    window_items(ui, Some(mode), &displays, &mut menu_act);
                });
                if ui.input(|i| i.viewport().close_requested()) {
                    action = Some(None);
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) && matches!(mode, WinMode::Full(_)) {
                    action = Some(Some(WinMode::Window));
                }
            });
            if let Some(a) = menu_act {
                self.run_menu_action(out, a);
            }
            if let Some(a) = action {
                self.set_out_win(out, a);
            }
        }
    }
}
