use crate::app::{App, RightTab};
use crate::audio::{device_names, Controls, Meters};
use crate::model::*;
use crate::theme::{self, *};
use eframe::egui::{self, Color32, RichText, Sense, Vec2};

const MUTED_FILL: Color32 = Color32::from_rgb(92, 30, 36);

fn quiet(level: f32) -> bool {
    level_db(level) < -50.0
}

fn sound_pill(ui: &mut egui::Ui, label: &str, short: &str, level: f32, state: PillState, compact: bool) -> egui::Response {
    let name = if compact { short } else { label };
    let text = match state {
        PillState::Muted => format!("{name} muted"),
        PillState::Off => format!("{name} off"),
        PillState::On => name.to_string(),
    };
    let font = egui::FontId::new(12.5, semi_family());
    let color = match state {
        PillState::Muted => Color32::from_rgb(255, 170, 170),
        PillState::Off => GOLD,
        PillState::On => TEXT,
    };
    let galley = ui.painter().layout_no_wrap(text, font, color);
    let width = (galley.size().x + 20.0).max(if compact { 60.0 } else { 96.0 });
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
    let hovered = resp.hovered();
    let p = ui.painter();
    let fill = match state {
        PillState::Muted => MUTED_FILL,
        PillState::Off => Color32::from_rgb(58, 46, 26),
        PillState::On if hovered => BTN_HOVER,
        PillState::On => BTN,
    };
    p.rect_filled(rect, 8, fill);
    p.galley(rect.left_top() + Vec2::new(10.0, 5.0), galley, color);
    let bar = egui::Rect::from_min_max(egui::pos2(rect.left() + 10.0, rect.bottom() - 8.0), egui::pos2(rect.right() - 10.0, rect.bottom() - 5.0));
    paint_meter(p, bar, level, state != PillState::On);
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

#[derive(Clone, Copy, PartialEq)]
enum PillState {
    On,
    Muted,
    Off,
}

fn route_chip(ui: &mut egui::Ui, on: bool, label: &str, tip: &str) -> bool {
    chip(ui, on, label).on_hover_text(tip).clicked()
}

fn volume_slider(ui: &mut egui::Ui, gain: &mut f32, max: f32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Volume").size(12.0).color(MUTED));
        ui.spacing_mut().slider_width = (ui.available_width() - 8.0).max(60.0);
        ui.add(egui::Slider::new(gain, 0.0..=max).show_value(false));
    });
}

#[allow(clippy::too_many_arguments)]
fn mixer_row(ui: &mut egui::Ui, name: &str, level: f32, gain: &mut f32, max: f32, stream: &mut bool, room: Option<&mut bool>, what: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).family(semi_family()).size(13.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            match room {
                Some(room) => {
                    if route_chip(ui, *room, "Room", &format!("Play {what} on the room speakers")) {
                        *room = !*room;
                    }
                }
                None => {
                    ui.add_enabled(false, egui::Button::new(RichText::new("Room").size(12.5)).corner_radius(14))
                        .on_disabled_hover_text("The microphone never plays in the room, so it can't feed back through the speakers");
                }
            }
            if route_chip(ui, *stream, "Stream", &format!("Send {what} to the stream and recording")) {
                *stream = !*stream;
            }
        });
    });
    meter(ui, level, ui.available_width());
    volume_slider(ui, gain, max);
    ui.add_space(4.0);
}

impl App {
    pub fn on_air_now(&self) -> bool {
        self.live.is_some() || self.rec.is_some()
    }

    fn mic_listening(&self) -> bool {
        self.sh.audio.mic_status.lock().unwrap().starts_with("Listening")
    }

    pub fn start_listening(&mut self) {
        self.sh.audio.set_input(Some(self.settings.mic_device.clone()));
        self.mic_open = true;
    }

    pub fn refresh_audio_devices(&mut self) {
        if self.audio_devices_rx.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send((device_names(true), device_names(false)));
        });
        self.audio_devices_rx = Some(rx);
    }

    pub fn poll_audio_devices(&mut self) {
        let Some(rx) = &self.audio_devices_rx else { return };
        if let Ok((mics, outs)) = rx.try_recv() {
            self.mic_devices = mics;
            self.out_devices = outs;
            self.audio_devices_rx = None;
        }
    }

    fn set_controls(&self, ctl: Controls) {
        let mut c = self.sh.audio.controls.lock().unwrap();
        let break_mute = c.break_mute_mic;
        *c = ctl;
        c.break_mute_mic = break_mute;
    }

    pub fn sound_strip(&mut self, ui: &mut egui::Ui, compact: bool) {
        let meters: Meters = *self.sh.audio.meters.lock().unwrap();
        let mut ctl = *self.sh.audio.controls.lock().unwrap();
        let before = format!("{ctl:?}");
        let on_air = self.on_air_now();
        let listening = self.mic_listening();
        ui.vertical(|ui| {
            ui.add_space(4.0);
            let (title, color) = if on_air { ("ON-AIR SOUND", RED) } else { ("STREAM SOUND", MUTED) };
            ui.label(RichText::new(title).family(semi_family()).size(11.5).color(color));
            ui.horizontal(|ui| {
                let mic_state = if !listening {
                    PillState::Off
                } else if !ctl.mic_on || ctl.break_mute_mic {
                    PillState::Muted
                } else {
                    PillState::On
                };
                let mic_tip = match mic_state {
                    PillState::Off => "The microphone isn't listening. Click to start it.".to_string(),
                    PillState::Muted if ctl.break_mute_mic => "Muted for the break. It comes back when the break ends.".to_string(),
                    PillState::Muted => "Muted on the stream and recording. Click to unmute.".to_string(),
                    PillState::On if quiet(meters.mic) => "On the stream, but no sound is coming in. Check the mic or the input in Sound.".to_string(),
                    PillState::On => "On the stream and recording. Click to mute.".to_string(),
                };
                if sound_pill(ui, "Microphone", "Mic", meters.mic, mic_state, compact).on_hover_text(mic_tip).clicked() {
                    if listening {
                        ctl.mic_on = !ctl.mic_on;
                    } else {
                        self.start_listening();
                    }
                }
                let computer = meters.music.max(meters.media);
                let comp_state = if ctl.computer_on_stream() { PillState::On } else { PillState::Muted };
                let comp_tip = if ctl.computer_on_stream() { "Music and video sound go to the stream and recording. Click to mute them there (the room still hears them)." } else { "Music and video sound are muted on the stream and recording. Click to unmute." };
                if sound_pill(ui, "Music & video", "Media", computer, comp_state, compact).on_hover_text(comp_tip).clicked() {
                    let on = !ctl.computer_on_stream();
                    ctl.set_computer_on_stream(on);
                }
            });
        });
        if format!("{ctl:?}") != before {
            self.set_controls(ctl);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
    }

    pub fn sound_tab(&mut self, ui: &mut egui::Ui) {
        if self.mic_devices.is_empty() && self.out_devices.is_empty() {
            self.refresh_audio_devices();
        }
        let meters = *self.sh.audio.meters.lock().unwrap();
        let mut ctl = *self.sh.audio.controls.lock().unwrap();
        let before = format!("{ctl:?}");
        let mut opened = None;
        let tab = RightTab::Sound;

        let open = self.section_open(tab, "Microphone");
        let mic_status = self.sh.audio.mic_status.lock().unwrap().clone();
        let mut new_mic: Option<String> = None;
        let mut refresh = false;
        let mut retry = false;
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let summary = if ctl.break_mute_mic { "Muted on the stream for the break".to_string() } else { mic_status.clone() };
            let head = section_head(ui, open, "Microphone", None, &summary, |ui| {
                let (label, fill, fg) = if ctl.mic_on { ("ON", GREEN, INK) } else { ("MUTED", RED, Color32::WHITE) };
                if ui.add(egui::Button::new(RichText::new(label).family(bold_family()).color(fg)).fill(fill).min_size(Vec2::new(64.0, 26.0))).on_hover_text("On or muted on the stream and recording").clicked() {
                    ctl.mic_on = !ctl.mic_on;
                }
            });
            if head {
                opened = Some("Microphone");
            }
            meter(ui, meters.mic, ui.available_width());
            if !open {
                return;
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Input").size(12.0).color(MUTED));
                let current = if self.settings.mic_device.is_empty() { "Computer's default input".to_string() } else { self.settings.mic_device.clone() };
                let mut pick = self.settings.mic_device.clone();
                egui::ComboBox::from_id_salt("mic_input").width(ui.available_width() - 70.0).selected_text(current).show_ui(ui, |ui| {
                    ui.selectable_value(&mut pick, String::new(), "Computer's default input");
                    for d in &self.mic_devices {
                        ui.selectable_value(&mut pick, d.clone(), d);
                    }
                });
                if pick != self.settings.mic_device {
                    new_mic = Some(pick);
                }
                if small_button(ui, "↻").on_hover_text("Look for inputs again, for example after plugging in a USB mixer").clicked() {
                    refresh = true;
                }
            });
            hint(ui, "Pick the sound desk, USB mixer or microphone that carries the service sound. It goes to the stream and recording only, never to the room speakers.");
            ui.horizontal(|ui| {
                ui.label(RichText::new("Level").size(12.0).color(MUTED));
                ui.add(egui::Slider::new(&mut ctl.mic_gain, 0.0..=3.0).show_value(false));
            });
            let problem = mic_status.starts_with("Microphone problem");
            ui.horizontal(|ui| {
                ui.label(RichText::new(&mic_status).size(12.0).color(if problem { GOLD } else { MUTED }));
                if (problem || !self.mic_listening()) && small_button(ui, "Try again").clicked() {
                    retry = true;
                }
            });
            if self.mic_listening() && quiet(meters.mic) {
                hint(ui, "No sound is coming in. Check the cable or volume on the desk, or pick another input.");
            }
            if ctl.break_mute_mic {
                ui.label(RichText::new("Muted on the stream for the break").size(12.0).color(GOLD));
            }
        });
        if let Some(m) = new_mic {
            self.settings.mic_device = m;
            self.start_listening();
        }
        if retry {
            self.start_listening();
        }
        if refresh {
            self.refresh_audio_devices();
        }

        ui.add_space(6.0);
        let open = self.section_open(tab, "Music");
        let playing = self.sh.audio.is_playing("music");
        let name = self.music_idx.and_then(|i| self.lib.playlist.get(i)).map(|p| file_name(p)).unwrap_or_else(|| "Nothing playing".into());
        let mut music_action: Option<i32> = None;
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let summary = if playing { format!("♪ {name}") } else { format!("{} track(s) in the music list", self.lib.playlist.len()) };
            let head = section_head(ui, open, "Music", None, &summary, |ui| {
                if playing {
                    if small_button(ui, "■ Fade out").clicked() {
                        music_action = Some(0);
                    }
                } else if small_button(ui, "▶ Play").clicked() {
                    music_action = Some(1);
                }
            });
            if head {
                opened = Some("Music");
            }
            meter(ui, meters.music, ui.available_width());
            if !open {
                return;
            }
            ui.label(RichText::new(if playing { format!("♪ {name}") } else { name.clone() }).size(13.0));
            if let Some(pos) = self.sh.audio.position("music") {
                ui.label(RichText::new(fmt_time(pos)).size(12.0).color(MUTED));
            }
            ui.horizontal(|ui| {
                if small_button(ui, "⏮").on_hover_text("Previous track").clicked() {
                    music_action = Some(-1);
                }
                if small_button(ui, "⏭").on_hover_text("Next track").clicked() {
                    music_action = Some(2);
                }
            });
            hint(ui, &format!("{} track(s) in the music list. Volume and where it plays are in Outputs.", self.lib.playlist.len()));
        });
        match music_action {
            Some(0) => self.sh.audio.stop("music", 2.0),
            Some(1) if self.lib.playlist.is_empty() => self.toast("Add music in Media or Breaks & ads first", true),
            Some(1) => self.play_music(self.music_idx.unwrap_or(0)),
            Some(-1) => {
                let i = self.music_idx.unwrap_or(0);
                let n = self.lib.playlist.len().max(1);
                self.play_music((i + n - 1) % n);
            }
            Some(2) => self.play_music(self.music_idx.map(|i| i + 1).unwrap_or(0)),
            _ => {}
        }

        ui.add_space(6.0);
        let open = self.section_open(tab, "Outputs");
        let mut new_out: Option<String> = None;
        let out_status = self.sh.audio.out_status.lock().unwrap().clone();
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut stream_parts = Vec::new();
            if ctl.mic_on {
                stream_parts.push("mic");
            }
            if ctl.music_stream {
                stream_parts.push("music");
            }
            if ctl.media_stream {
                stream_parts.push("video");
            }
            let stream_text = if stream_parts.is_empty() { "nothing".to_string() } else { stream_parts.join(", ") };
            let room_text = if !ctl.monitor_on { "off".to_string() } else { [("music", ctl.music_room), ("video", ctl.media_room)].iter().filter(|(_, on)| *on).map(|(n, _)| *n).collect::<Vec<_>>().join(", ") };
            let summary = format!("Stream: {stream_text} · Room: {}", if room_text.is_empty() { "nothing" } else { &room_text });
            if section_head(ui, open, "Outputs", None, &summary, |_| {}) {
                opened = Some("Outputs");
            }
            meter(ui, meters.program, ui.available_width());
            if !open {
                return;
            }
            subheading(ui, "Sounds");
            mixer_row(ui, "Microphone", meters.mic, &mut ctl.mic_gain, 3.0, &mut ctl.mic_on, None, "the microphone");
            mixer_row(ui, "Music", meters.music, &mut ctl.music_gain, 1.5, &mut ctl.music_stream, Some(&mut ctl.music_room), "break music");
            mixer_row(ui, "Video sound", meters.media, &mut ctl.media_gain, 1.5, &mut ctl.media_stream, Some(&mut ctl.media_room), "video sound");
            ui.add_space(6.0);
            subheading(ui, "Stream & recording");
            meter(ui, meters.program, ui.available_width());
            volume_slider(ui, &mut ctl.master, 2.0);
            ui.add_space(6.0);
            subheading(ui, "Room speakers");
            ui.horizontal(|ui| {
                ui.checkbox(&mut ctl.monitor_on, "On");
                let current = if self.settings.output_device.is_empty() { "Computer's default speakers".to_string() } else { self.settings.output_device.clone() };
                let mut pick = self.settings.output_device.clone();
                egui::ComboBox::from_id_salt("room_out").width(ui.available_width() - 12.0).selected_text(current).show_ui(ui, |ui| {
                    ui.selectable_value(&mut pick, String::new(), "Computer's default speakers");
                    for d in &self.out_devices {
                        ui.selectable_value(&mut pick, d.clone(), d);
                    }
                });
                if pick != self.settings.output_device {
                    new_out = Some(pick);
                }
            });
            meter(ui, meters.room, ui.available_width());
            volume_slider(ui, &mut ctl.room_gain, 1.5);
            hint(ui, &out_status);
        });
        if let Some(o) = new_out {
            self.settings.output_device = o.clone();
            self.sh.audio.set_output(if o.is_empty() { None } else { Some(o) });
        }
        if let Some(name) = opened {
            self.toggle_section(tab, name);
        }
        if format!("{ctl:?}") != before {
            self.set_controls(ctl);
            self.settings.monitor_in_room = ctl.monitor_on;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
    }
}
