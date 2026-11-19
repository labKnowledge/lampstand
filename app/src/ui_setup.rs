use crate::app::{list_devices, App, SetupTab};
use crate::bible::{abbrev_from_name, Translation};
use crate::model::*;
use crate::services::{service, upload_needed_kbps, SERVICES};
use crate::store;
use crate::stream::check_destination;
use crate::theme::{self, *};
use eframe::egui::{self, Color32, RichText, Vec2};

impl App {
    pub fn setup_window(&mut self, ctx: &egui::Context) {
        if !self.show_setup {
            return;
        }
        let mut open = true;
        egui::Window::new(RichText::new("Stream setup").family(heading_family()).size(20.0))
            .open(&mut open)
            .collapsible(false)
            .default_width(640.0)
            .default_height(620.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (t, l) in [(SetupTab::Destinations, "Where to stream"), (SetupTab::Quality, "Quality & FFmpeg"), (SetupTab::Devices, "Sound & screens"), (SetupTab::Bible, "Church & Bible")] {
                        if chip(ui, self.setup_tab == t, l).clicked() {
                            self.setup_tab = t;
                            if t == SetupTab::Devices && self.mic_devices.is_empty() {
                                list_devices(self);
                            }
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().id_salt("setup").max_height(560.0).show(ui, |ui| match self.setup_tab {
                    SetupTab::Destinations => self.destinations_ui(ui),
                    SetupTab::Quality => self.quality_ui(ui),
                    SetupTab::Devices => self.devices_ui(ui),
                    SetupTab::Bible => self.church_ui(ui),
                });
            });
        if !open {
            self.show_setup = false;
            self.persist();
        }
    }

    fn destinations_ui(&mut self, ui: &mut egui::Ui) {
        hint(ui, "Switch on every place you want to go live. Lampstand sends one picture to all of them at the same time.");
        ui.add_space(4.0);
        subheading(ui, "Add a destination");
        ui.horizontal_wrapped(|ui| {
            for s in SERVICES.iter() {
                let b = egui::Button::new(RichText::new(s.name).family(semi_family()).size(13.0).color(Color32::WHITE)).fill(rgb(s.color)).corner_radius(14);
                if ui.add(b).clicked() {
                    let n = self.settings.destinations.iter().filter(|d| d.service == s.id).count();
                    let name = if n == 0 { s.name.to_string() } else { format!("{} {}", s.name, n + 1) };
                    self.settings.destinations.push(Destination { service: s.id.into(), name, server: s.server.into(), key: String::new(), enabled: true });
                }
            }
        });
        ui.add_space(8.0);
        if self.settings.destinations.is_empty() {
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("No destinations yet").family(semi_family()));
                hint(ui, "Click YouTube or Facebook above, then paste the stream key from that site.");
            });
        }
        let mut remove = None;
        let n = self.settings.destinations.len();
        for i in 0..n {
            let svc = service(&self.settings.destinations[i].service);
            let check = self.checks.get(&i).cloned();
            let mut do_check = false;
            theme::card_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                let d = &mut self.settings.destinations[i];
                ui.horizontal(|ui| {
                    ui.checkbox(&mut d.enabled, "");
                    let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 5.0, rgb(svc.color));
                    ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(180.0).font(egui::FontId::new(15.0, semi_family())));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if small_button(ui, "Remove").clicked() {
                            remove = Some(i);
                        }
                        if small_button(ui, "Check connection").on_hover_text("Tests that this computer can reach the platform's server. Nothing is broadcast.").clicked() {
                            do_check = true;
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Server").size(12.5).color(MUTED));
                    if svc.fixed_server && d.server == svc.server {
                        ui.label(RichText::new(&d.server).size(12.5));
                        if small_button(ui, "Change").on_hover_text("Only change this if the platform shows a different address").clicked() {
                            d.server.push(' ');
                        }
                    } else {
                        let edit = egui::TextEdit::singleline(&mut d.server).desired_width(ui.available_width()).hint_text("rtmp://… or rtmps://…");
                        ui.add(edit);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Stream key").size(12.5).color(MUTED));
                    let reveal = self.reveal_key == Some(i);
                    ui.add(egui::TextEdit::singleline(&mut d.key).password(!reveal).desired_width(ui.available_width() - 60.0).hint_text("paste the key here"));
                    if small_button(ui, if reveal { "Hide" } else { "Show" }).clicked() {
                        self.reveal_key = if reveal { None } else { Some(i) };
                    }
                });
                ui.label(RichText::new(svc.where_key).size(12.5).color(TEXT));
                ui.label(RichText::new(svc.tips).size(12.0).color(MUTED));
                match &check {
                    Some(None) => {
                        ui.label(RichText::new("Checking…").size(12.0).color(MUTED));
                    }
                    Some(Some(Ok(m))) => {
                        ui.label(RichText::new(format!("✔ {m}")).size(12.0).color(GREEN));
                    }
                    Some(Some(Err(e))) => {
                        ui.label(RichText::new(format!("✖ {e}")).size(12.0).color(RED));
                    }
                    None => {}
                }
                if self.settings.bitrate_kbps > svc.max_kbps {
                    ui.label(RichText::new(format!("{} accepts up to {} kbps. Lower the quality in the next tab.", svc.name, svc.max_kbps)).size(12.0).color(GOLD));
                }
            });
            if do_check {
                let url = self.settings.destinations[i].server.clone();
                self.checks.insert(i, None);
                let tx = self.check_tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send((i, check_destination(&url)));
                });
            }
            ui.add_space(4.0);
        }
        if let Some(i) = remove {
            self.settings.destinations.remove(i);
            self.checks.clear();
        }
        let ready = self.ready_destinations().len();
        ui.add_space(6.0);
        let need = upload_needed_kbps(self.settings.bitrate_kbps, ready);
        hint(ui, &format!("{ready} destination(s) ready. Your internet upload should be at least {:.1} Mbps for this.", need as f32 / 1000.0));
        if ready > 2 {
            hint(ui, "Tip: with a slow connection, send one stream to Restream and let it copy to the other platforms.");
        }
    }

    fn quality_ui(&mut self, ui: &mut egui::Ui) {
        subheading(ui, "Picture");
        let mut q = self.settings.quality;
        for opt in [Quality::Hd720, Quality::Hd1080] {
            ui.radio_value(&mut q, opt, opt.name());
        }
        if q != self.settings.quality {
            if self.live.is_some() || self.rec.is_some() {
                self.toast("Stop the stream and recording before changing the picture size", true);
            } else {
                self.settings.quality = q;
                self.settings.bitrate_kbps = if q == Quality::Hd1080 { 6000 } else { 4000 };
                let size = q.size();
                self.sh.edit(|s| s.canvas = size);
            }
        }
        ui.horizontal(|ui| {
            ui.label("Bitrate");
            ui.add(egui::Slider::new(&mut self.settings.bitrate_kbps, 1500..=9000).step_by(250.0).suffix(" kbps"));
        });
        hint(ui, "720p: 3000 to 4500 kbps. 1080p: 4500 to 6000 kbps. Use less if viewers see buffering.");
        ui.add_space(8.0);
        subheading(ui, "Encoder");
        let encs: Vec<String> = self.ff.as_ref().map(|f| f.encoders.clone()).unwrap_or_default();
        let friendly = |e: &str| match e {
            "libx264" => "Software (works on every computer)",
            "h264_videotoolbox" => "Apple hardware (Mac, light on the computer)",
            "h264_nvenc" => "NVIDIA graphics card",
            "h264_qsv" => "Intel Quick Sync",
            "h264_amf" => "AMD graphics card",
            "h264_vaapi" => "Linux hardware (VA-API)",
            _ => "Other",
        };
        let cur = self.settings.encoder.clone();
        egui::ComboBox::from_id_salt("encoder").width(320.0).selected_text(friendly(&cur)).show_ui(ui, |ui| {
            for e in &encs {
                if e != "h264_vaapi" {
                    ui.selectable_value(&mut self.settings.encoder, e.clone(), friendly(e));
                }
            }
        });
        ui.add_space(8.0);
        subheading(ui, "Recording");
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.settings.record_dir).size(12.5).color(MUTED));
            if small_button(ui, "Change folder…").clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    self.settings.record_dir = d.to_string_lossy().to_string();
                }
            }
        });
        ui.checkbox(&mut self.settings.record_with_live, "Record automatically whenever we go live");
        ui.add_space(8.0);
        subheading(ui, "FFmpeg (the video engine)");
        match &self.ff {
            Some(f) => {
                ui.label(RichText::new(format!("✔ Found: {}", f.path.display())).size(12.5).color(GREEN));
                ui.label(RichText::new(&f.version).size(12.0).color(MUTED));
            }
            None => {
                ui.label(RichText::new("FFmpeg is not installed yet. Lampstand uses it to stream, record, play videos and music, and read cameras.").size(13.0).color(GOLD));
                let how = if cfg!(target_os = "macos") {
                    "On a Mac: install Homebrew from brew.sh, then run:  brew install ffmpeg"
                } else if cfg!(windows) {
                    "On Windows: open PowerShell and run:  winget install ffmpeg   (or download it from ffmpeg.org and choose ffmpeg.exe below)"
                } else {
                    "On Linux: run  sudo apt install ffmpeg  (or your distribution's package manager)"
                };
                ui.label(RichText::new(how).size(12.5));
            }
        }
        ui.horizontal(|ui| {
            if small_button(ui, "Choose FFmpeg file…").clicked() {
                if let Some(f) = rfd::FileDialog::new().pick_file() {
                    self.settings.ffmpeg_path = f.to_string_lossy().to_string();
                    self.relocate_ffmpeg();
                }
            }
            if small_button(ui, "Look again").clicked() {
                self.relocate_ffmpeg();
                let msg = if self.ff.is_some() { "FFmpeg found" } else { "Still can't find FFmpeg" };
                self.toast(msg, self.ff.is_none());
            }
        });
        if let Some(l) = &self.live {
            ui.add_space(8.0);
            subheading(ui, "Live encoder messages");
            let s = l.stats.lock().unwrap().clone();
            for line in s.log.iter().rev().take(8) {
                ui.label(RichText::new(line).size(11.5).color(MUTED));
            }
        }
    }

    fn devices_ui(&mut self, ui: &mut egui::Ui) {
        subheading(ui, "Microphone or sound desk input");
        let mut mic = self.settings.mic_device.clone();
        egui::ComboBox::from_id_salt("mic").width(340.0).selected_text(if mic.is_empty() { "System default".to_string() } else { mic.clone() }).show_ui(ui, |ui| {
            ui.selectable_value(&mut mic, String::new(), "System default");
            for d in &self.mic_devices {
                ui.selectable_value(&mut mic, d.clone(), d);
            }
        });
        if mic != self.settings.mic_device {
            self.settings.mic_device = mic.clone();
            if self.mic_open {
                self.sh.audio.set_input(Some(mic));
            }
        }
        ui.add_space(6.0);
        subheading(ui, "Room speakers (music and video sound)");
        let mut out = self.settings.output_device.clone();
        egui::ComboBox::from_id_salt("out").width(340.0).selected_text(if out.is_empty() { "System default".to_string() } else { out.clone() }).show_ui(ui, |ui| {
            ui.selectable_value(&mut out, String::new(), "System default");
            for d in &self.out_devices {
                ui.selectable_value(&mut out, d.clone(), d);
            }
        });
        if out != self.settings.output_device {
            self.settings.output_device = out.clone();
            self.sh.audio.set_output(if out.is_empty() { None } else { Some(out) });
        }
        if small_button(ui, "Refresh device list").clicked() {
            list_devices(self);
        }
        ui.add_space(10.0);
        subheading(ui, "Projector");
        ui.horizontal(|ui| {
            ui.label("Open the projector full screen on display");
            ui.add(egui::DragValue::new(&mut self.settings.projector_display).range(1..=8));
        });
        hint(ui, "Display 1 is usually the main computer screen; the projector or TV is usually display 2. Double-click the projector picture or press Esc to switch between window and full screen.");
    }

    fn church_ui(&mut self, ui: &mut egui::Ui) {
        subheading(ui, "Church name");
        if ui.add(egui::TextEdit::singleline(&mut self.settings.church_name).desired_width(320.0)).changed() {
            let n = self.settings.church_name.clone();
            self.sh.edit(|s| s.logo.text = n);
        }
        ui.add_space(8.0);
        subheading(ui, "Bibles");
        for t in &self.bibles.translations {
            ui.label(RichText::new(format!("{} · {} ({} verses)", t.abbrev, t.name, t.verses.len())).size(13.0));
        }
        hint(ui, "The World English Bible and King James Version are public domain and built in. Many more public domain Bibles in other languages are free from eBible.org: download the 'VPL' text file, then add it here.");
        if small_button(ui, "Add a Bible file (VPL .txt)…").clicked() {
            if let Some(f) = rfd::FileDialog::new().add_filter("Verse-per-line Bible", &["txt"]).pick_file() {
                match std::fs::read_to_string(&f) {
                    Ok(text) => {
                        let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "bible".into());
                        let abbrev = abbrev_from_name(&stem);
                        match Translation::from_vpl(&abbrev, &stem, &text) {
                            Some(t) => {
                                let _ = std::fs::create_dir_all(store::bibles_dir());
                                let _ = std::fs::write(store::bibles_dir().join(format!("{stem}.txt")), &text);
                                self.import_msg = format!("Added {} with {} verses", t.abbrev, t.verses.len());
                                self.bibles.translations.retain(|x| x.abbrev != t.abbrev);
                                self.bibles.translations.push(t);
                            }
                            None => self.import_msg = "That file doesn't look like a verse-per-line Bible".into(),
                        }
                    }
                    Err(e) => self.import_msg = format!("Couldn't read the file: {e}"),
                }
            }
        }
        if !self.import_msg.is_empty() {
            ui.label(RichText::new(&self.import_msg).size(12.5).color(GOLD));
        }
        hint(ui, "Copyrighted translations (for example NIV or ESV) need permission from their publisher before you project or stream them.");
    }
}
