use crate::audio::SrcKind;
use crate::bible::BibleStore;
use crate::engine::Shared;
use crate::media::{locate_ffmpeg, CameraDevice, FfmpegInfo};
use crate::model::*;
use crate::store;
use crate::stream::{self, Sink, SinkState, StartSpec};
use crate::theme::{self, *};
use crate::LaunchOptions;
use eframe::egui::{self, Color32, ColorImage, Key, Margin, RichText, Sense, Stroke, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LeftTab {
    Bible,
    Songs,
    Media,
    Breaks,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CenterTab {
    Slides,
    Sources,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum RightTab {
    Graphics,
    Sound,
    Look,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SetupTab {
    Destinations,
    Quality,
    Devices,
    Bible,
}

pub struct Strip {
    pub title: String,
    pub slides: Vec<Slide>,
    pub live: Option<usize>,
    pub focus: Option<usize>,
    pub scroll: bool,
}

pub struct Toast {
    pub text: String,
    pub until: f64,
    pub error: bool,
}

pub struct App {
    pub sh: Arc<Shared>,
    pub settings: Settings,
    pub lib: Library,
    pub bibles: BibleStore,
    pub ff: Option<FfmpegInfo>,
    pub tex: [Option<TextureHandle>; 2],
    pub last_seq: u64,
    pub left_tab: LeftTab,
    pub right_tab: RightTab,
    pub look_tab: OutputId,
    pub send_target: SendTarget,
    pub transition: Transition,
    pub strip: Strip,
    pub bible_input: String,
    pub bible_search: String,
    pub bible_results: Vec<usize>,
    pub bible_book: u8,
    pub bible_chapter: u16,
    pub bible_recent: Vec<String>,
    pub selected_song: Option<usize>,
    pub song_filter: String,
    pub editing: Option<(Option<usize>, Song)>,
    pub cameras: Vec<CameraDevice>,
    pub caption_input: String,
    pub caption_pos: usize,
    pub new_ad_title: String,
    pub new_ad_body: String,
    pub new_ad_color: Rgb,
    pub music_idx: Option<usize>,
    pub live: Option<Arc<Sink>>,
    pub rec: Option<Arc<Sink>>,
    pub show_setup: bool,
    pub setup_tab: SetupTab,
    pub out_win: [Option<crate::ui_outputs::WinMode>; 2],
    pub display_list: Arc<Mutex<Vec<crate::capture::DisplayInfo>>>,
    pub main_rect: Option<egui::Rect>,
    pub screen_asked: bool,
    pub confirm_end: bool,
    pub toasts: Vec<Toast>,
    pub shot_path: Option<PathBuf>,
    pub shot_image: Arc<Mutex<Option<Arc<ColorImage>>>>,
    pub shot_requested: bool,
    pub frames: u64,
    pub last_save: f64,
    pub check_tx: Sender<(usize, Result<String, String>)>,
    pub check_rx: Receiver<(usize, Result<String, String>)>,
    pub checks: HashMap<usize, Option<Result<String, String>>>,
    pub reveal_key: Option<usize>,
    pub mic_devices: Vec<String>,
    pub out_devices: Vec<String>,
    pub mic_open: bool,
    pub audio_devices_rx: Option<Receiver<(Vec<String>, Vec<String>)>>,
    pub top_right_width: f32,
    pub import_msg: String,
    pub center_tab: CenterTab,
    pub show_library: bool,
    pub sources_scanned: bool,
    pub camera_scan: Option<Receiver<crate::ui_sources::CameraScan>>,
    pub stuck_cameras: Vec<crate::media::StuckCamera>,
    pub camera_restarts: HashMap<String, u8>,
    pub camera_needs_access: bool,
    pub camera_access_rx: Option<Receiver<Result<(), String>>>,
    pub camera_scan_at: f64,
    pub camera_scan_manual: bool,
    pub picked_screens: Vec<CameraDevice>,
    pub fresh: bool,
    pub pixels_per_point: f32,
    pub music_failures: usize,
    pub open_section: HashMap<RightTab, &'static str>,
    pub preview_src: Option<Background>,
    pub source_tex: HashMap<String, crate::ui_sources::SourceTex>,
    pub show_picker: bool,
    pub picker_items: Vec<crate::capture::CaptureTarget>,
    pub picker_note: String,
    pub picker_filter: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, opts: LaunchOptions) -> Self {
        theme::install(&cc.egui_ctx);
        let saved = if opts.fresh || opts.screenshot.is_some() { store::Loaded::Nothing } else { store::load() };
        let mut set_aside = None;
        let (settings, lib, mut state) = match saved {
            store::Loaded::Saved(s) => (s.settings, s.library, s.state),
            store::Loaded::Unreadable(kept) => {
                set_aside = Some(kept);
                (Settings::default(), Library::sample(), LiveState::default())
            }
            store::Loaded::Nothing => (Settings::default(), Library::sample(), LiveState::default()),
        };
        state.canvas = settings.quality.size();
        state.logo.text = if state.logo.text.is_empty() { settings.church_name.clone() } else { state.logo.text.clone() };
        let ff = locate_ffmpeg(&settings.ffmpeg_path);
        let sh = Shared::new(state, ff.as_ref().map(|f| f.path.clone()));
        *sh.ctx.lock().unwrap() = Some(cc.egui_ctx.clone());
        sh.start();
        if opts.screenshot.is_none() {
            let out = if settings.output_device.is_empty() { None } else { Some(settings.output_device.clone()) };
            sh.audio.start(out);
            sh.audio.controls.lock().unwrap().monitor_on = settings.monitor_in_room;
        }
        let bibles = BibleStore::load(Some(store::bibles_dir()));
        let (check_tx, check_rx) = std::sync::mpsc::channel();
        let mut app = App {
            sh,
            settings,
            lib,
            bibles,
            ff,
            tex: [None, None],
            last_seq: 0,
            left_tab: LeftTab::Bible,
            right_tab: RightTab::Graphics,
            look_tab: OutputId::Screen,
            send_target: SendTarget::Both,
            transition: Transition::Fade,
            strip: Strip { title: String::new(), slides: vec![], live: None, focus: None, scroll: false },
            bible_input: "John 3:16".into(),
            bible_search: String::new(),
            bible_results: vec![],
            bible_book: 42,
            bible_chapter: 3,
            bible_recent: vec![],
            selected_song: None,
            song_filter: String::new(),
            editing: None,
            cameras: vec![],
            caption_input: String::new(),
            caption_pos: 0,
            new_ad_title: String::new(),
            new_ad_body: String::new(),
            new_ad_color: [40, 90, 150],
            music_idx: None,
            live: None,
            rec: None,
            show_setup: false,
            setup_tab: SetupTab::Destinations,
            out_win: [None, None],
            display_list: Arc::new(Mutex::new((1..=3).map(|i| crate::capture::DisplayInfo { name: format!("Display {i}"), rect: None }).collect())),
            main_rect: None,
            screen_asked: false,
            confirm_end: false,
            toasts: vec![],
            shot_path: opts.screenshot.clone(),
            shot_image: Arc::new(Mutex::new(None)),
            shot_requested: false,
            frames: 0,
            last_save: now(),
            check_tx,
            check_rx,
            checks: HashMap::new(),
            reveal_key: None,
            mic_devices: vec![],
            out_devices: vec![],
            mic_open: false,
            audio_devices_rx: None,
            top_right_width: 750.0,
            import_msg: String::new(),
            center_tab: CenterTab::Slides,
            show_library: true,
            sources_scanned: false,
            camera_scan: None,
            stuck_cameras: vec![],
            camera_restarts: HashMap::new(),
            camera_needs_access: false,
            camera_access_rx: None,
            camera_scan_at: f64::MIN,
            camera_scan_manual: false,
            picked_screens: vec![],
            fresh: opts.fresh,
            pixels_per_point: cc.egui_ctx.pixels_per_point(),
            music_failures: 0,
            open_section: HashMap::from([(RightTab::Graphics, "Lower third"), (RightTab::Sound, "Microphone"), (RightTab::Look, "Text")]),
            preview_src: None,
            source_tex: HashMap::new(),
            show_picker: false,
            picker_items: vec![],
            picker_note: String::new(),
            picker_filter: String::new(),
        };
        if let Some(kept) = set_aside {
            app.toast(format!("Lampstand couldn't read its saved songs and settings, so it started fresh. The old file is kept at {}", kept.display()), true);
        }
        app.load_chapter(42, 3, Some(16), None, false);
        if app.shot_path.is_none() {
            app.refresh_displays();
            app.start_listening();
            app.refresh_audio_devices();
        }
        match opts.projector.as_deref() {
            Some("window") => app.out_win[0] = Some(crate::ui_outputs::WinMode::Window),
            Some("mini") => app.out_win[0] = Some(crate::ui_outputs::WinMode::Mini),
            Some("full") => {
                *app.display_list.lock().unwrap() = crate::capture::list_displays();
                app.toggle_projector();
            }
            _ => {}
        }
        if opts.sources {
            app.center_tab = CenterTab::Sources;
        }
        let welcome = Slide::text(SlideKind::Title, format!("Welcome to {}\nWe're glad you're here", app.settings.church_name), "");
        app.sh.edit(|s| s.send_slide(SendTarget::Both, Some(welcome), Transition::Cut));
        if app.shot_path.is_some() {
            app.sh.edit(|s| {
                s.lower_third.show.set(true);
                s.lower_third.show.t0 = -10.0;
                s.logo.show.set(true);
                s.logo.show.t0 = -10.0;
            });
        }
        app
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast { text: text.into(), until: now() + if error { 7.0 } else { 3.5 }, error });
    }

    pub fn persist(&mut self) {
        if self.shot_path.is_some() || self.fresh {
            return;
        }
        let saved = store::Saved { settings: self.settings.clone(), library: self.lib.clone(), state: self.sh.snapshot() };
        store::save(&saved);
        self.last_save = now();
    }

    pub fn send(&mut self, slide: Option<Slide>) {
        let target = self.send_target;
        let tr = self.transition;
        if let Some(Slide { media: Some(MediaRef::Video { path, sound, looping: false }), .. }) = &slide {
            let st = self.sh.snapshot();
            if st.outputs.iter().any(|o| o.slide == slide) {
                self.sh.videos.restart(&crate::media::video_key(path, false, *sound));
            }
        }
        self.sh.edit(|s| s.send_slide(target, slide, tr));
    }

    pub fn send_background(&mut self, bg: Background) {
        let target = self.send_target;
        let tr = self.transition;
        self.sh.edit(|s| s.send_background(target, bg, tr));
    }

    pub fn show_strip(&mut self, i: usize) {
        if let Some(s) = self.strip.slides.get(i).cloned() {
            self.strip.live = Some(i);
            self.strip.focus = Some(i);
            self.strip.scroll = true;
            self.send(Some(s));
        }
    }

    pub fn step(&mut self, d: i32) {
        if self.strip.slides.is_empty() {
            return;
        }
        let cur = self.strip.live.or(self.strip.focus).map(|x| x as i32).unwrap_or(-1);
        let n = (cur + d).clamp(0, self.strip.slides.len() as i32 - 1) as usize;
        self.show_strip(n);
    }

    pub fn set_strip(&mut self, title: String, slides: Vec<Slide>, focus: Option<usize>) {
        self.strip = Strip { title, slides, live: None, focus, scroll: focus.is_some() };
    }

    pub fn ffmpeg_path(&self) -> Option<PathBuf> {
        self.ff.as_ref().map(|f| f.path.clone())
    }

    pub fn relocate_ffmpeg(&mut self) {
        self.ff = locate_ffmpeg(&self.settings.ffmpeg_path);
        *self.sh.ffmpeg.write().unwrap() = self.ffmpeg_path();
    }

    pub fn ensure_mic(&mut self) {
        let on = self.sh.audio.controls.lock().unwrap().mic_on;
        if on && !self.mic_open {
            self.sh.audio.set_input(Some(self.settings.mic_device.clone()));
            self.mic_open = true;
        }
    }

    pub fn ready_destinations(&self) -> Vec<Destination> {
        self.settings.destinations.iter().filter(|d| d.enabled && !d.key.trim().is_empty() && d.server.contains("://")).cloned().collect()
    }

    pub fn go_live(&mut self) {
        let Some(ff) = self.ffmpeg_path() else {
            self.toast("Lampstand needs FFmpeg to stream. Open Stream setup to add it.", true);
            self.show_setup = true;
            self.setup_tab = SetupTab::Quality;
            return;
        };
        let dests = self.ready_destinations();
        if dests.is_empty() {
            self.toast("Add a destination and paste its stream key first.", true);
            self.show_setup = true;
            self.setup_tab = SetupTab::Destinations;
            return;
        }
        self.ensure_mic();
        let spec = StartSpec {
            ffmpeg: &ff,
            canvas: self.sh.snapshot().canvas,
            quality: self.settings.quality,
            kbps: self.settings.bitrate_kbps,
            encoder: &self.settings.encoder.clone(),
            output: stream::live_output_args(&dests),
            label: format!("Live on {}", dests.iter().map(|d| d.name.clone()).collect::<Vec<_>>().join(", ")),
            dest_names: dests.iter().map(|d| d.name.clone()).collect(),
            secrets: dests.iter().map(|d| d.key.trim().to_string()).collect(),
            path: None,
            stats_period: self.ff.as_ref().map(|f| f.has_stats_period()).unwrap_or(true),
        };
        match stream::start_sink(spec, &self.sh.audio) {
            Ok(s) => {
                self.sh.sinks.lock().unwrap().push(s.clone());
                self.live = Some(s);
                self.sh.edit(|st| st.is_live = true);
                self.toast(format!("Going live on {} destination(s)", dests.len()), false);
                if self.settings.record_with_live && self.rec.as_ref().map(|r| !r.is_active()).unwrap_or(true) {
                    self.start_recording();
                }
            }
            Err(e) => self.toast(e, true),
        }
    }

    pub fn end_live(&mut self) {
        if let Some(s) = self.live.take() {
            s.stop(&self.sh.audio);
        }
        self.sh.edit(|st| st.is_live = false);
        if self.settings.record_with_live {
            self.stop_recording();
        }
        self.toast("The live stream has ended", false);
    }

    pub fn start_recording(&mut self) {
        let Some(ff) = self.ffmpeg_path() else {
            self.toast("Recording needs FFmpeg. Open Stream setup to add it.", true);
            self.show_setup = true;
            self.setup_tab = SetupTab::Quality;
            return;
        };
        let dir = PathBuf::from(&self.settings.record_dir);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.toast(format!("Can't use the recording folder: {e}"), true);
            return;
        }
        self.ensure_mic();
        let name = format!("Service {}.mp4", chrono::Local::now().format("%Y-%m-%d %H-%M-%S"));
        let path = dir.join(name);
        let spec = StartSpec {
            ffmpeg: &ff,
            canvas: self.sh.snapshot().canvas,
            quality: self.settings.quality,
            kbps: (self.settings.bitrate_kbps * 2).max(6000),
            encoder: &self.settings.encoder.clone(),
            output: stream::record_output_args(&path),
            label: "Recording".into(),
            dest_names: vec![],
            secrets: vec![],
            path: Some(path.clone()),
            stats_period: self.ff.as_ref().map(|f| f.has_stats_period()).unwrap_or(true),
        };
        match stream::start_sink(spec, &self.sh.audio) {
            Ok(s) => {
                self.sh.sinks.lock().unwrap().push(s.clone());
                self.rec = Some(s);
                self.toast(format!("Recording to {}", path.display()), false);
            }
            Err(e) => self.toast(e, true),
        }
    }

    pub fn stop_recording(&mut self) {
        if let Some(s) = self.rec.take() {
            s.stop(&self.sh.audio);
            if let Some(p) = &s.path {
                self.toast(format!("Recording saved: {}", p.display()), false);
            }
        }
    }

    pub fn start_break(&mut self) {
        let st = self.sh.snapshot();
        let b = st.brk.clone();
        self.sh.edit(|s| {
            s.brk.show.set(true);
            s.brk.ends_at = if b.countdown_min > 0.0 { Some(now() + b.countdown_min as f64 * 60.0) } else { None };
            s.caption.show.set(false);
        });
        if b.mute_mic {
            self.sh.audio.controls.lock().unwrap().break_mute_mic = true;
        }
        if b.pause_recording {
            if let Some(r) = &self.rec {
                r.set_paused(true);
            }
        }
        if b.music && !self.lib.playlist.is_empty() && !self.sh.audio.is_playing("music") {
            self.play_music(self.music_idx.unwrap_or(0));
        }
    }

    pub fn end_break(&mut self) {
        let b = self.sh.snapshot().brk;
        self.sh.edit(|s| s.brk.show.set(false));
        self.sh.audio.controls.lock().unwrap().break_mute_mic = false;
        if let Some(r) = &self.rec {
            r.set_paused(false);
        }
        if b.music {
            self.sh.audio.stop("music", 2.5);
        }
    }

    pub fn play_music(&mut self, i: usize) {
        if self.lib.playlist.is_empty() {
            return;
        }
        let i = i % self.lib.playlist.len();
        let path = self.lib.playlist[i].clone();
        self.music_idx = Some(i);
        self.sh.audio.play("music", &path, SrcKind::Music, false, 1.0, 1.2);
        if self.ff.is_none() {
            self.toast("Music needs FFmpeg to play. Open Stream setup to add it.", true);
        }
    }

    pub fn add_files(&mut self, paths: Vec<PathBuf>) {
        let mut added = 0;
        for p in paths {
            let s = p.to_string_lossy().to_string();
            if let Some(kind) = media_kind_for(&s) {
                if !self.lib.media.iter().any(|m| m.path == s) {
                    self.lib.media.push(MediaFile { path: s, kind });
                    added += 1;
                }
            }
        }
        if added > 0 {
            self.toast(format!("Added {added} file(s) to Media"), false);
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        self.frames += 1;
        let (seq, w, h, screen, streamf) = {
            let f = self.sh.frames.lock().unwrap();
            (f.seq, f.w, f.h, f.screen.clone(), f.stream.clone())
        };
        if seq != self.last_seq || self.tex[0].is_none() {
            self.last_seq = seq;
            for (i, data) in [screen, streamf].iter().enumerate() {
                if data.len() == (w * h * 4) as usize {
                    let img = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], data);
                    match &mut self.tex[i] {
                        Some(t) => t.set(img, TextureOptions::LINEAR),
                        None => self.tex[i] = Some(ctx.load_texture(if i == 0 { "screen" } else { "stream" }, img, TextureOptions::LINEAR)),
                    }
                }
            }
        }
        for (k, secs) in self.sh.audio.take_finished() {
            if k != "music" {
                continue;
            }
            self.music_failures = if secs < 1.0 { self.music_failures + 1 } else { 0 };
            if self.music_failures >= self.lib.playlist.len().max(1) {
                self.music_failures = 0;
                self.toast("Couldn't play the break music. Check the music files are still on this computer.", true);
            } else if let Some(i) = self.music_idx {
                self.play_music(i + 1);
            }
        }
        self.poll_camera_scan(ctx);
        self.poll_audio_devices();
        while let Ok((i, r)) = self.check_rx.try_recv() {
            self.checks.insert(i, Some(r));
        }
        if let Some(msg) = self.live.as_ref().and_then(|l| sink_end_reason(l)) {
            if let Some(l) = self.live.take() {
                l.stop(&self.sh.audio);
            }
            self.sh.edit(|s| s.is_live = false);
            self.toast(format!("The live stream stopped: {msg}"), true);
        }
        if let Some(msg) = self.rec.as_ref().and_then(|r| sink_end_reason(r)) {
            if let Some(r) = self.rec.take() {
                r.stop(&self.sh.audio);
                if let Some(p) = &r.path {
                    self.toast(format!("Recording stopped: {msg}. What was recorded is saved in {}", p.display()), true);
                }
            }
        }
        let t = now();
        self.toasts.retain(|x| x.until > t);
        if t - self.last_save > 20.0 {
            self.persist();
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        if !typing && self.close_top_dialog_on_escape(ctx) {
            return;
        }
        if !typing && self.editing.is_none() && !self.confirm_end {
            let (next, prev, black, clear) = ctx.input(|i| {
                (
                    i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::Space) || i.key_pressed(Key::PageDown),
                    i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::PageUp),
                    i.key_pressed(Key::B),
                    i.key_pressed(Key::Escape),
                )
            });
            if ctx.input(|i| i.key_pressed(Key::C)) {
                self.toggle_clean(None);
            }
            if next {
                self.step(1);
            }
            if prev {
                self.step(-1);
            }
            if black {
                self.toggle_black(None);
            }
            if clear {
                self.strip.live = None;
                self.send(None);
            }
        }
    }

    fn close_top_dialog_on_escape(&mut self, ctx: &egui::Context) -> bool {
        let open = self.confirm_end || self.show_picker || self.show_setup;
        if !open || !ctx.input(|i| i.key_pressed(Key::Escape)) {
            return false;
        }
        if self.confirm_end {
            self.confirm_end = false;
        } else if self.show_picker {
            self.show_picker = false;
        } else {
            self.show_setup = false;
            self.persist();
        }
        true
    }

    pub fn toggle_black(&mut self, only: Option<OutputId>) {
        let target = self.send_target;
        self.sh.edit(|s| {
            let mirror = s.mirror;
            for o in [OutputId::Screen, OutputId::Stream] {
                let applies = match only {
                    Some(x) => mirror || x == o,
                    None => mirror || target.includes(o),
                };
                if applies {
                    let b = !s.out(o).black;
                    s.out_mut(o).set_black(b);
                }
            }
        });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let t = now();
        let narrow = ui.available_width() < NARROW_TOP_BAR;
        ui.horizontal_centered(|ui| {
            ui.add_space(4.0);
            let (r, _) = ui.allocate_exact_size(Vec2::new(26.0, 30.0), Sense::hover());
            let p = ui.painter();
            p.circle_filled(egui::pos2(r.center().x, r.top() + 9.0), 6.5, GOLD);
            p.circle_filled(egui::pos2(r.center().x, r.top() + 10.5), 3.0, Color32::from_rgb(255, 240, 200));
            p.rect_filled(egui::Rect::from_center_size(egui::pos2(r.center().x, r.top() + 21.0), Vec2::new(4.0, 10.0)), 1, GOLD);
            p.rect_filled(egui::Rect::from_center_size(egui::pos2(r.center().x, r.bottom() - 2.0), Vec2::new(16.0, 3.0)), 1, GOLD);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Lampstand").family(heading_family()).size(22.0).color(TEXT));
                ui.label(RichText::new(&self.settings.church_name).size(11.5).color(MUTED));
            });
            ui.add_space(18.0);
            ui.separator();
            ui.add_space(6.0);
            let mirror = self.sh.snapshot().mirror;
            ui.vertical(|ui| {
                ui.add_space(4.0);
                subheading(ui, "Screen & stream");
                ui.horizontal(|ui| {
                    if chip(ui, mirror, "Same picture").on_hover_text("The stream shows exactly what is on the church screen").clicked() {
                        self.sh.edit(|s| s.set_mirror(true));
                    }
                    if chip(ui, !mirror, "Different pictures").on_hover_text("Choose separately what the church screen and the stream show").clicked() {
                        self.sh.edit(|s| s.set_mirror(false));
                    }
                });
            });
            ui.add_space(6.0);
            if !narrow {
            ui.vertical(|ui| {
                ui.add_space(4.0);
                subheading(ui, "Transition");
                let tr = &mut self.transition;
                egui::ComboBox::from_id_salt("transition").width(96.0).selected_text(tr.name()).show_ui(ui, |ui| {
                    for x in Transition::ALL {
                        ui.selectable_value(tr, x, x.name());
                    }
                });
            });
            }
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            let room = ui.available_width() - self.top_right_width - 12.0;
            if room > 120.0 {
                self.sound_strip(ui, room < 235.0);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                if theme::small_button(ui, "Stream setup").clicked() {
                    self.show_setup = true;
                }
                let lib_toggle = if narrow {
                    ui.add(egui::Button::new(RichText::new("Library").size(13.0)).selected(self.show_library).min_size(Vec2::new(0.0, 26.0)))
                } else {
                    theme::small_button(ui, if self.show_library { "Hide library" } else { "Show library" })
                };
                if lib_toggle.on_hover_text("Show or hide the Bible and library column to give the monitors more room").clicked() {
                    self.show_library = !self.show_library;
                }
                ui.add_space(4.0);
                match self.live.clone() {
                    Some(l) => {
                        let st = l.stats.lock().unwrap().clone();
                        let label = match st.state {
                            SinkState::Starting => "CONNECTING…".to_string(),
                            _ => format!("END LIVE  {}", fmt_time(st.elapsed(t))),
                        };
                        if big_button(ui, &label, RED, Color32::WHITE, Vec2::new(if narrow { 0.0 } else { 150.0 }, 38.0)).clicked() {
                            self.confirm_end = true;
                        }
                    }
                    None => {
                        let n = self.ready_destinations().len();
                        let label = if n > 1 { format!("GO LIVE ({n})") } else { "GO LIVE".to_string() };
                        if big_button(ui, &label, Color32::from_rgb(196, 46, 52), Color32::WHITE, Vec2::new(if narrow { 0.0 } else { 128.0 }, 38.0))
                            .on_hover_text("Start streaming to every destination that is switched on")
                            .clicked()
                        {
                            self.go_live();
                        }
                    }
                }
                ui.add_space(4.0);
                match self.rec.clone() {
                    Some(r) => {
                        let st = r.stats.lock().unwrap().clone();
                        let paused = r.paused.load(std::sync::atomic::Ordering::SeqCst);
                        if big_button(ui, "■ STOP", CARD_HI, TEXT, Vec2::new(78.0, 38.0)).clicked() {
                            self.stop_recording();
                        }
                        let pl = if paused { "▶ RESUME" } else { "❚❚ PAUSE" };
                        if big_button(ui, pl, CARD_HI, TEXT, Vec2::new(92.0, 38.0)).on_hover_text("Pause or resume the recording file").clicked() {
                            r.set_paused(!paused);
                        }
                        let c = if paused { GOLD } else { RED };
                        ui.label(RichText::new(format!("● REC {}", fmt_time(st.elapsed(t)))).family(bold_family()).color(c));
                    }
                    None => {
                        if big_button(ui, if narrow { "● REC" } else { "● RECORD" }, CARD_HI, TEXT, Vec2::new(0.0, 38.0)).on_hover_text("Record the stream picture and sound to a video file").clicked() {
                            self.start_recording();
                        }
                    }
                }
                ui.add_space(8.0);
                let in_break = self.sh.snapshot().brk.show.visible;
                if in_break {
                    let ends = self.sh.snapshot().brk.ends_at;
                    let label = match ends {
                        Some(e) => format!("END BREAK  {}", crate::render::countdown_text(e - t)),
                        None => "END BREAK".into(),
                    };
                    if big_button(ui, &label, GOLD, INK, Vec2::new(150.0, 38.0)).clicked() {
                        self.end_break();
                    }
                } else if big_button(ui, if narrow { "☕ BREAK" } else { "☕ START BREAK" }, CARD_HI, GOLD, Vec2::new(0.0, 38.0))
                    .on_hover_text("Show the break screen with ads and a countdown, and play break music")
                    .clicked()
                {
                    self.start_break();
                }
                ui.add_space(4.0);
                let pl = match (self.out_win[0].is_some(), narrow) {
                    (true, false) => "Close projector",
                    (false, false) => "Open projector",
                    (true, true) => "Close proj.",
                    (false, true) => "Projector",
                };
                let projector = big_button(ui, pl, CARD_HI, TEXT, Vec2::new(0.0, 38.0));
                self.top_right_width = ui.min_rect().width();
                if projector
                    .on_hover_text("Show the church screen full screen on the projector or TV. Right-click a preview for more choices.")
                    .clicked()
                {
                    self.toggle_projector();
                }
            });
        });
    }

    fn monitor(&mut self, ui: &mut egui::Ui, out: OutputId, width: f32) {
        let st = self.sh.snapshot();
        let o = if st.mirror { &st.outputs[0] } else { st.out(out) };
        let t = now();
        let win_text = crate::ui_outputs::describe(self.out_win[out.index()], &self.displays());
        card_frame().inner_margin(Margin::same(8)).show(ui, |ui| {
            ui.set_width(width - 18.0);
            ui.horizontal(|ui| {
                let stream_sub = match (self.live.is_some(), self.out_win[1].is_some()) {
                    (true, true) => format!("On air · {win_text}"),
                    (true, false) => "On air".to_string(),
                    (false, true) => format!("Not live · {win_text}"),
                    (false, false) => "Not live".to_string(),
                };
                let (title, sub) = match out {
                    OutputId::Screen => ("In-church screen", if self.out_win[0].is_some() { win_text.clone() } else { "Projector closed · right-click to show".to_string() }),
                    OutputId::Stream => ("Live stream & recording", stream_sub),
                };
                ui.label(RichText::new(title).family(heading_family()).size(16.0));
                let c = match out {
                    OutputId::Screen => if self.out_win[0].is_some() { GREEN } else { MUTED },
                    OutputId::Stream => if self.live.is_some() { RED } else { MUTED },
                };
                status_dot(ui, c);
                ui.label(RichText::new(sub).size(12.0).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if o.black {
                        ui.label(RichText::new("BLACK").family(bold_family()).size(11.0).color(GOLD));
                    }
                    if st.brk.show.visible && st.brk.place.on(out) {
                        ui.label(RichText::new("BREAK").family(bold_family()).size(11.0).color(GOLD));
                    }
                });
            });
            let w = ui.available_width();
            let h = w * 9.0 / 16.0;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
            let p = ui.painter();
            p.rect_filled(rect, 4, Color32::BLACK);
            if let Some(tex) = &self.tex[out.index()] {
                p.image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
            let border = if out == OutputId::Stream && self.live.is_some() { RED } else { LINE };
            p.rect_stroke(rect, 4, Stroke::new(if border == RED { 2.0 } else { 1.0 }, border), egui::StrokeKind::Outside);
            if resp.double_clicked() {
                if out == OutputId::Screen {
                    self.toggle_projector();
                } else if self.out_win[1].is_none() {
                    self.set_out_win(out, Some(crate::ui_outputs::WinMode::Window));
                }
            }
            let resp = resp.on_hover_text("Right-click for display, window and mini-window choices");
            resp.context_menu(|ui| self.output_menu(ui, out));
            ui.horizontal_wrapped(|ui| {
                if small_button(ui, "Clear text").on_hover_text("Esc").clicked() {
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
                if small_button(ui, "Clear background").clicked() {
                    let tr = self.transition;
                    self.sh.edit(|s| {
                        let bg = Background::Gradient(WARM_BG.0, WARM_BG.1);
                        if s.mirror {
                            s.send_background(SendTarget::Both, bg, tr);
                        } else {
                            s.out_mut(out).set_background(bg, tr);
                        }
                    });
                }
                let black = o.black;
                if ui.add(egui::Button::new(RichText::new(if black { "Unblack" } else { "Black" }).size(13.0)).selected(black)).on_hover_text("B").clicked() {
                    self.toggle_black(Some(out));
                }
                let clean = o.clean;
                if ui
                    .add(egui::Button::new(RichText::new(if clean { "Show text & graphics" } else { "Clear view" }).size(13.0)).selected(clean))
                    .on_hover_text("Hide Bible text, lyrics and graphics so only the camera or background shows. Click again to bring them back. Shortcut: C")
                    .clicked()
                {
                    self.toggle_clean(Some(out));
                }
            });
            let _ = t;
        });
    }

    fn center(&mut self, ui: &mut egui::Ui) {
        let mirror = self.sh.snapshot().mirror;
        let avail = ui.available_width();
        ui.horizontal(|ui| {
            let w = (avail - 12.0) / 2.0;
            ui.vertical(|ui| {
                ui.set_width(w);
                self.monitor(ui, OutputId::Screen, w);
            });
            ui.vertical(|ui| {
                ui.set_width(w);
                self.monitor(ui, OutputId::Stream, w);
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let title = if self.strip.title.is_empty() { "Slides".to_string() } else { format!("Slides · {}", self.strip.title) };
            if chip(ui, self.center_tab == CenterTab::Slides, &title).clicked() {
                self.center_tab = CenterTab::Slides;
            }
            if chip(ui, self.center_tab == CenterTab::Sources, "🎥 Cameras & screens").on_hover_text("Preview cameras and screen capture, then put them on the screen or stream").clicked() {
                self.center_tab = CenterTab::Sources;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !mirror {
                    for (t, label) in [(SendTarget::Stream, "Stream only"), (SendTarget::Screen, "Screen only"), (SendTarget::Both, "Both")] {
                        if chip(ui, self.send_target == t, label).clicked() {
                            self.send_target = t;
                        }
                    }
                    ui.label(RichText::new("Clicks go to").size(12.0).color(MUTED));
                } else if self.center_tab == CenterTab::Slides {
                    hint(ui, "Click a slide to show it. Arrow keys or Space step through.");
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Slides show as").size(12.0).color(MUTED));
            let cur = self.current_layout();
            for l in TextLayout::ALL {
                if chip(ui, cur == l, l.name()).on_hover_text(l.help()).clicked() {
                    self.set_layout(l);
                }
            }
        });
        ui.add_space(2.0);
        if self.center_tab == CenterTab::Sources {
            self.sources_panel(ui);
            return;
        }
        let slides = self.strip.slides.clone();
        if slides.is_empty() {
            card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                hint(ui, "Pick a Bible passage, a song or announcement, or media from the library on the left. Its slides appear here.");
            });
            return;
        }
        let mut clicked = None;
        egui::ScrollArea::vertical().id_salt("strip").auto_shrink([false, false]).show(ui, |ui| {
            let cw = 214.0;
            let ch = 112.0;
            let cols = ((ui.available_width() + 8.0) / (cw + 8.0)).floor().max(1.0) as usize;
            let cw = (ui.available_width() - (cols as f32 - 1.0) * 8.0) / cols as f32;
            for row in slides.chunks(cols).enumerate() {
                ui.horizontal(|ui| {
                    for (j, s) in row.1.iter().enumerate() {
                        let i = row.0 * cols + j;
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(cw, ch), Sense::click());
                        let live = self.strip.live == Some(i);
                        let focus = self.strip.focus == Some(i);
                        let p = ui.painter();
                        let fill = if live { Color32::from_rgb(58, 46, 26) } else if resp.hovered() { CARD_HI } else { CARD };
                        p.rect_filled(rect, 8, fill);
                        let stroke = if live { Stroke::new(2.0, GOLD) } else if focus { Stroke::new(1.0, GOLD_DIM) } else { Stroke::new(1.0, LINE) };
                        p.rect_stroke(rect, 8, stroke, egui::StrokeKind::Inside);
                        let label = if s.label.is_empty() { format!("{}", i + 1) } else { s.label.clone() };
                        p.text(rect.left_top() + Vec2::new(10.0, 8.0), egui::Align2::LEFT_TOP, &label, egui::FontId::new(11.5, semi_family()), if live { GOLD } else { MUTED });
                        if live {
                            p.text(rect.right_top() + Vec2::new(-10.0, 8.0), egui::Align2::RIGHT_TOP, "LIVE", egui::FontId::new(11.0, bold_family()), GOLD);
                        }
                        let body = if s.kind == SlideKind::Media { format!("▶ {}", s.label) } else { s.text.clone() };
                        let galley = ui.painter().layout(body, egui::FontId::new(13.0, egui::FontFamily::Proportional), TEXT, cw - 20.0);
                        let clip = egui::Rect::from_min_max(rect.left_top() + Vec2::new(10.0, 28.0), rect.right_bottom() - Vec2::new(10.0, 8.0));
                        ui.painter().with_clip_rect(clip).galley(clip.left_top(), galley, TEXT);
                        if resp.clicked() {
                            clicked = Some(i);
                        }
                        if focus && self.strip.scroll {
                            resp.scroll_to_me(Some(egui::Align::Center));
                            self.strip.scroll = false;
                        }
                    }
                });
                ui.add_space(2.0);
            }
        });
        if let Some(i) = clicked {
            self.show_strip(i);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            let es = *self.sh.stats.lock().unwrap();
            match &self.ff {
                Some(f) => {
                    status_dot(ui, GREEN);
                    ui.label(RichText::new(format!("FFmpeg ready ({})", f.version.split_whitespace().nth(2).unwrap_or(""))).size(12.0).color(MUTED));
                }
                None => {
                    status_dot(ui, GOLD);
                    if ui.link(RichText::new("FFmpeg not found: streaming, recording, video and music are off. Click to set up.").size(12.0)).clicked() {
                        self.show_setup = true;
                        self.setup_tab = SetupTab::Quality;
                    }
                }
            }
            ui.separator();
            let st = self.sh.snapshot();
            ui.label(RichText::new(format!("{}×{} · {:.0} fps · draw {:.1} ms", st.canvas.0, st.canvas.1, es.fps, es.render_ms)).size(12.0).color(MUTED));
            if let Some(l) = &self.live {
                let s = l.stats.lock().unwrap().clone();
                ui.separator();
                status_dot(ui, if s.state == SinkState::Running { RED } else { GOLD });
                let failed: Vec<String> = s.failed.iter().filter_map(|i| l.dest_names.get(*i).cloned()).collect();
                let failed = if failed.is_empty() { String::new() } else { format!(" · lost: {}", failed.join(", ")) };
                let dropped = l.dropped_video.load(std::sync::atomic::Ordering::Relaxed) + l.dropped_audio.load(std::sync::atomic::Ordering::Relaxed);
                let drop_txt = if dropped > 0 { format!(" · {dropped} dropped") } else { String::new() };
                let rate = if s.kbps > 0.0 { format!(" · {:.0} kbps", s.kbps) } else { String::new() };
                ui.label(RichText::new(format!("{}{} · {:.2}x{}{}", l.label, rate, s.speed, drop_txt, failed)).size(12.0).color(if failed.is_empty() { MUTED } else { GOLD }));
            }
            let errs = self.sh.videos.errors.lock().unwrap().clone();
            if let Some((_, e)) = errs.iter().next() {
                ui.separator();
                ui.label(RichText::new(format!("Media: {}", e.chars().take(90).collect::<String>())).size(12.0).color(GOLD));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("→ next  ← back  B black  Esc clear").size(12.0).color(MUTED));
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.confirm_end {
            let mut keep = true;
            let modal = egui::Modal::new(egui::Id::unique("confirm_end")).show(ctx, |ui| {
                ui.set_width(360.0);
                heading(ui, "End the live stream?");
                hint(ui, "Viewers on every platform will see the stream end.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if big_button(ui, "End live", RED, Color32::WHITE, Vec2::new(110.0, 34.0)).clicked() {
                        self.end_live();
                        keep = false;
                    }
                    if big_button(ui, "Keep streaming", CARD_HI, TEXT, Vec2::new(140.0, 34.0)).clicked() {
                        keep = false;
                    }
                });
            });
            if modal.backdrop_response.clicked() {
                keep = false;
            }
            self.confirm_end = keep && self.confirm_end;
        }
        if !self.toasts.is_empty() {
            egui::Area::new(egui::Id::unique("toasts")).anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -40.0)).show(ctx, |ui| {
                for t in &self.toasts {
                    egui::Frame::new()
                        .fill(if t.error { Color32::from_rgb(70, 28, 32) } else { CARD_HI })
                        .stroke(Stroke::new(1.0, if t.error { RED } else { GOLD_DIM }))
                        .corner_radius(8)
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.label(RichText::new(&t.text).size(13.0));
                        });
                    ui.add_space(6.0);
                }
            });
        }
    }

    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot_path.clone() else { return };
        ctx.request_repaint();
        if self.frames >= 120 && !self.shot_requested {
            self.shot_requested = true;
            let slot = self.shot_image.clone();
            ctx.request_screenshot(move |img| {
                *slot.lock().unwrap() = Some(img);
            });
        }
        let got = self.shot_image.lock().unwrap().take();
        if let Some(img) = got {
            let [w, h] = img.size;
            let mut bytes = Vec::with_capacity(w * h * 4);
            for c in &img.pixels {
                bytes.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
            }
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = buf.save(&path);
            }
            *self.sh.preview.lock().unwrap() = HashMap::new();
            self.sh.videos.stop_all();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.pixels_per_point = ctx.pixels_per_point();
        self.poll(&ctx);
        self.update_previews(&ctx);
        if let Some(r) = ctx.input(|i| i.viewport().outer_rect) {
            self.main_rect = Some(r);
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if !dropped.is_empty() {
            self.add_files(dropped);
            self.left_tab = LeftTab::Media;
        }
        egui::Panel::top("top").exact_size(62.0).frame(egui::Frame::new().fill(BG).inner_margin(Margin::symmetric(10, 4))).show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").exact_size(28.0).frame(egui::Frame::new().fill(BG).inner_margin(Margin::symmetric(10, 2))).show(ui, |ui| self.status_bar(ui));
        if self.show_library {
            egui::Panel::left("library").default_size(372.0).size_range(320.0..=520.0).frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::same(12))).show(ui, |ui| self.library(ui));
        }
        egui::Panel::right("graphics").default_size(352.0).size_range(320.0..=480.0).frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::same(12))).show(ui, |ui| self.right_panel(ui));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG).inner_margin(Margin::same(12))).show(ui, |ui| self.center(ui));
        self.setup_window(&ctx);
        self.song_editor(&ctx);
        self.capture_picker(&ctx);
        self.output_windows(&ctx);
        self.dialogs(&ctx);
        self.screenshot(&ctx);
        if self.live.is_some() || self.rec.is_some() || self.sh.audio.is_playing("music") {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.mic_devices.truncate(64);
    }

    fn on_exit(&mut self) {
        self.persist();
        if let Some(s) = self.live.take() {
            s.stop(&self.sh.audio);
        }
        if let Some(s) = self.rec.take() {
            s.stop(&self.sh.audio);
        }
        self.sh.videos.stop_all();
        std::thread::sleep(std::time::Duration::from_millis(600));
    }
}

fn sink_end_reason(sink: &Sink) -> Option<String> {
    match sink.stats.lock().unwrap().state.clone() {
        SinkState::Failed(msg) => Some(msg),
        SinkState::Ended => Some("the encoder closed".into()),
        _ => None,
    }
}

const NARROW_TOP_BAR: f32 = 1440.0;

pub fn list_devices(app: &mut App) {
    app.refresh_audio_devices();
}
