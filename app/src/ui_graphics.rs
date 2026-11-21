use crate::app::{App, RightTab};
use crate::model::*;
use crate::theme::{self, *};
use crate::ui_library::BG_PRESETS;
use eframe::egui::{self, Color32, RichText, Vec2};

fn where_chips(ui: &mut egui::Ui, w: &mut Where) {
    if chip(ui, w.screen, "Screen").on_hover_text("Show on the in-church screen").clicked() {
        w.screen = !w.screen;
    }
    if chip(ui, w.stream, "Stream").on_hover_text("Show on the live stream and recording").clicked() {
        w.stream = !w.stream;
    }
}

fn show_button(ui: &mut egui::Ui, visible: bool) -> bool {
    let (label, fill, fg) = if visible { ("HIDE", GREEN, INK) } else { ("SHOW", CARD_HI, TEXT) };
    ui.add(egui::Button::new(RichText::new(label).family(bold_family()).size(12.5).color(fg)).fill(fill).min_size(Vec2::new(64.0, 26.0)).corner_radius(6)).clicked()
}

fn graphic_head(ui: &mut egui::Ui, open: bool, title: &str, visible: bool, summary: &str, opened: &mut bool) -> bool {
    let mut shown = false;
    let dot = if visible { GREEN } else { Color32::from_rgb(70, 74, 88) };
    *opened = section_head(ui, open, title, Some(dot), summary, |ui| shown = show_button(ui, visible));
    shown
}

fn place_row(ui: &mut egui::Ui, place: &mut Where) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Show on").size(12.0).color(MUTED));
        where_chips(ui, place);
    });
}

fn short(text: &str, max: usize) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if line.chars().count() > max { format!("{}…", line.chars().take(max).collect::<String>()) } else { line.to_string() }
}

impl App {
    pub fn section_open(&self, tab: RightTab, name: &str) -> bool {
        self.open_section.get(&tab).is_some_and(|n| *n == name)
    }

    pub fn toggle_section(&mut self, tab: RightTab, name: &'static str) {
        if self.section_open(tab, name) {
            self.open_section.remove(&tab);
        } else {
            self.open_section.insert(tab, name);
        }
    }

    pub fn right_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (tab, label) in [(RightTab::Graphics, "On-air graphics"), (RightTab::Sound, "Sound"), (RightTab::Look, "Look & effects")] {
                if chip(ui, self.right_tab == tab, label).clicked() {
                    self.right_tab = tab;
                }
            }
        });
        ui.add_space(8.0);
        egui::ScrollArea::vertical().id_salt("right").auto_shrink([false, false]).show(ui, |ui| match self.right_tab {
            RightTab::Graphics => self.graphics_tab(ui),
            RightTab::Sound => self.sound_tab(ui),
            RightTab::Look => self.look_tab_ui(ui),
        });
    }

    fn graphics_tab(&mut self, ui: &mut egui::Ui) {
        let st = self.sh.snapshot();

        let mut lt = st.lower_third.clone();
        let mut lt_toggle = false;
        let mut save_person = false;
        let mut opened = None;
        let open = self.section_open(RightTab::Graphics, "Lower third");
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let summary = if lt.title.is_empty() { lt.name.clone() } else { format!("{} · {}", lt.name, lt.title) };
            let mut o = false;
            lt_toggle = graphic_head(ui, open, "Lower third", lt.show.visible, &summary, &mut o);
            if o {
                opened = Some("Lower third");
            }
            if !open {
                return;
            }
            place_row(ui, &mut lt.place);
            ui.horizontal_wrapped(|ui| {
                for p in self.lib.people.clone() {
                    let on = p.name == lt.name && p.title == lt.title;
                    if chip(ui, on, &p.name).on_hover_text(&p.title).clicked() {
                        lt.name = p.name.clone();
                        lt.title = p.title.clone();
                    }
                }
            });
            ui.add(egui::TextEdit::singleline(&mut lt.name).hint_text("Name").desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut lt.title).hint_text("Role or title").desired_width(ui.available_width() - 60.0));
                if small_button(ui, "Save").on_hover_text("Keep this person in the quick list").clicked() {
                    save_person = true;
                }
            });
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("lt_style").width(110.0).selected_text(lt.style.name()).show_ui(ui, |ui| {
                    for s in LtStyle::ALL {
                        ui.selectable_value(&mut lt.style, s, s.name());
                    }
                });
                egui::ComboBox::from_id_salt("lt_anim").width(90.0).selected_text(lt.anim.name()).show_ui(ui, |ui| {
                    for a in Anim::ALL {
                        ui.selectable_value(&mut lt.anim, a, a.name());
                    }
                });
                ui.color_edit_button_srgb(&mut lt.accent).on_hover_text("Accent colour");
            });
        });
        if save_person && !lt.name.trim().is_empty() {
            let p = Person { name: lt.name.trim().into(), title: lt.title.trim().into() };
            if !self.lib.people.contains(&p) {
                self.lib.people.push(p);
            }
        }

        ui.add_space(6.0);
        let mut tc = st.title.clone();
        let mut tc_toggle = false;
        let open = self.section_open(RightTab::Graphics, "Title");
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut o = false;
            tc_toggle = graphic_head(ui, open, "Title", tc.show.visible, &short(&tc.text, 60), &mut o);
            if o {
                opened = Some("Title");
            }
            if !open {
                return;
            }
            place_row(ui, &mut tc.place);
            ui.add(egui::TextEdit::singleline(&mut tc.text).hint_text("Main title, e.g. Sunday Service").desired_width(f32::INFINITY));
            ui.add(egui::TextEdit::singleline(&mut tc.sub).hint_text("Small line above, e.g. Sermon series").desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if chip(ui, !tc.centered, "Corner").clicked() {
                    tc.centered = false;
                }
                if chip(ui, tc.centered, "Centre").clicked() {
                    tc.centered = true;
                }
                egui::ComboBox::from_id_salt("tc_anim").width(90.0).selected_text(tc.anim.name()).show_ui(ui, |ui| {
                    for a in Anim::ALL {
                        ui.selectable_value(&mut tc.anim, a, a.name());
                    }
                });
            });
        });

        ui.add_space(6.0);
        let mut cap = st.caption.clone();
        let mut cap_toggle = false;
        let mut cap_send: Option<String> = None;
        let mut cap_clear = false;
        let open = self.section_open(RightTab::Graphics, "Captions");
        let script_lines = self.lib.caption_script.lines().filter(|l| !l.trim().is_empty()).count();
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let summary = match cap.lines.last() {
                Some(l) if cap.show.visible => short(l, 60),
                _ if script_lines > 0 => format!("{script_lines} lines ready in the prepared script"),
                _ => "Type live captions".into(),
            };
            let mut o = false;
            cap_toggle = graphic_head(ui, open, "Captions", cap.show.visible, &summary, &mut o);
            if o {
                opened = Some("Captions");
            }
            if !open {
                return;
            }
            place_row(ui, &mut cap.place);
            let r = ui.add(egui::TextEdit::singleline(&mut self.caption_input).hint_text("Type and press Enter to caption live").desired_width(f32::INFINITY));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !self.caption_input.trim().is_empty() {
                cap_send = Some(self.caption_input.trim().to_string());
                self.caption_input.clear();
                r.request_focus();
            }
            ui.horizontal(|ui| {
                ui.checkbox(&mut cap.large, "Large");
                ui.label(RichText::new("Clear after").size(12.0).color(MUTED));
                ui.add(egui::DragValue::new(&mut cap.auto_clear).range(0.0..=60.0).suffix(" s"));
                if small_button(ui, "Clear").clicked() {
                    cap_clear = true;
                }
            });
            egui::CollapsingHeader::new(RichText::new("Prepared script").size(13.0)).id_salt("script").show(ui, |ui| {
                hint(ui, "Paste sermon notes or song words, one caption per line. Then press Next line as you go.");
                ui.add(egui::TextEdit::multiline(&mut self.lib.caption_script).desired_rows(4).desired_width(f32::INFINITY));
                let lines: Vec<String> = self.lib.caption_script.lines().filter(|l| !l.trim().is_empty()).map(|l| l.trim().to_string()).collect();
                ui.horizontal(|ui| {
                    if small_button(ui, "Next line ▶").clicked() && !lines.is_empty() {
                        let i = self.caption_pos.min(lines.len() - 1);
                        cap_send = Some(lines[i].clone());
                        self.caption_pos = (i + 1).min(lines.len());
                    }
                    if small_button(ui, "Restart").clicked() {
                        self.caption_pos = 0;
                    }
                    ui.label(RichText::new(format!("{}/{}", self.caption_pos.min(lines.len()), lines.len())).size(12.0).color(MUTED));
                });
                if let Some(next) = lines.get(self.caption_pos) {
                    ui.label(RichText::new(format!("Next: {next}")).size(12.0).color(MUTED));
                }
            });
        });

        ui.add_space(6.0);
        let mut tk = st.ticker.clone();
        let mut tk_toggle = false;
        let open = self.section_open(RightTab::Graphics, "Ticker");
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut o = false;
            tk_toggle = graphic_head(ui, open, "Ticker", tk.show.visible, &short(&tk.text, 60), &mut o);
            if o {
                opened = Some("Ticker");
            }
            if !open {
                return;
            }
            place_row(ui, &mut tk.place);
            ui.add(egui::TextEdit::multiline(&mut tk.text).desired_rows(2).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Speed").size(12.0).color(MUTED));
                ui.add(egui::Slider::new(&mut tk.speed, 30.0..=240.0).show_value(false));
                ui.color_edit_button_srgb(&mut tk.color);
            });
        });

        ui.add_space(6.0);
        let mut lg = st.logo.clone();
        let mut lg_toggle = false;
        let open = self.section_open(RightTab::Graphics, "Logo & live badge");
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let summary = lg.image.as_ref().map(|p| file_name(p)).unwrap_or_else(|| lg.text.clone());
            let mut o = false;
            lg_toggle = graphic_head(ui, open, "Logo & live badge", lg.show.visible, &summary, &mut o);
            if o {
                opened = Some("Logo & live badge");
            }
            if !open {
                return;
            }
            place_row(ui, &mut lg.place);
            ui.add(egui::TextEdit::singleline(&mut lg.text).hint_text("Church name").desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.checkbox(&mut lg.live_badge, "Red LIVE badge while streaming");
            });
            ui.horizontal(|ui| {
                let label = lg.image.as_ref().map(|p| file_name(p)).unwrap_or_else(|| "No picture (uses the name)".into());
                ui.label(RichText::new(label).size(12.0).color(MUTED));
                if small_button(ui, "Picture…").clicked() {
                    if let Some(f) = rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "webp"]).pick_file() {
                        lg.image = Some(f.to_string_lossy().to_string());
                    }
                }
                if lg.image.is_some() && small_button(ui, "✕").clicked() {
                    lg.image = None;
                }
            });
        });

        if let Some(name) = opened {
            self.toggle_section(RightTab::Graphics, name);
        }
        let changed = format!("{:?}{:?}{:?}{:?}{:?}", lt, tc, cap, tk, lg) != format!("{:?}{:?}{:?}{:?}{:?}", st.lower_third, st.title, st.caption, st.ticker, st.logo);
        if changed || lt_toggle || tc_toggle || cap_toggle || tk_toggle || lg_toggle || cap_send.is_some() || cap_clear {
            self.sh.edit(|s| {
                let keep = (s.lower_third.show.clone(), s.title.show.clone(), s.caption.show.clone(), s.ticker.show.clone(), s.logo.show.clone(), s.caption.lines.clone(), s.caption.last_t);
                s.lower_third = lt;
                s.title = tc;
                s.caption = cap;
                s.ticker = tk;
                s.logo = lg;
                s.lower_third.show = keep.0;
                s.title.show = keep.1;
                s.caption.show = keep.2;
                s.ticker.show = keep.3;
                s.logo.show = keep.4;
                s.caption.lines = keep.5;
                s.caption.last_t = keep.6;
                if lt_toggle {
                    let v = !s.lower_third.show.visible;
                    s.lower_third.show.set(v);
                }
                if tc_toggle {
                    let v = !s.title.show.visible;
                    s.title.show.set(v);
                }
                if cap_toggle {
                    let v = !s.caption.show.visible;
                    s.caption.show.set(v);
                }
                if tk_toggle {
                    let v = !s.ticker.show.visible;
                    s.ticker.show.set(v);
                }
                if lg_toggle {
                    let v = !s.logo.show.visible;
                    s.logo.show.set(v);
                }
                if let Some(line) = cap_send {
                    s.caption.lines.push(line);
                    if s.caption.lines.len() > 20 {
                        s.caption.lines.remove(0);
                    }
                    s.caption.last_t = now();
                    s.caption.show.set(true);
                }
                if cap_clear {
                    s.caption.lines.clear();
                }
            });
        }
    }

    fn look_tab_ui(&mut self, ui: &mut egui::Ui) {
        let st = self.sh.snapshot();
        let idx = if st.mirror {
            hint(ui, "Screen and stream share one look. Choose Different pictures at the top to style them separately.");
            0
        } else {
            ui.horizontal(|ui| {
                if chip(ui, self.look_tab == OutputId::Screen, "Church screen").clicked() {
                    self.look_tab = OutputId::Screen;
                }
                if chip(ui, self.look_tab == OutputId::Stream, "Stream").clicked() {
                    self.look_tab = OutputId::Stream;
                }
            });
            self.look_tab.index()
        };
        let mut look = st.outputs[idx].look.clone();
        let before = format!("{:?}", look);
        let tab = RightTab::Look;
        let mut opened = None;
        let open = self.section_open(tab, "Text");
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if section_head(ui, open, "Text", None, &format!("{} · {}", look.font.name(), look.layout.name()), |_| {}) {
                opened = Some("Text");
            }
            if !open {
                return;
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("font").width(140.0).selected_text(look.font.name()).show_ui(ui, |ui| {
                    for f in FontChoice::ALL {
                        ui.selectable_value(&mut look.font, f, f.name());
                    }
                });
                ui.color_edit_button_srgb(&mut look.color);
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Size").size(12.0).color(MUTED));
                ui.add(egui::Slider::new(&mut look.size, 0.6..=1.6).show_value(false));
            });
            ui.horizontal_wrapped(|ui| {
                if chip(ui, look.shadow, "Shadow").clicked() {
                    look.shadow = !look.shadow;
                }
                if chip(ui, look.outline, "Outline").clicked() {
                    look.outline = !look.outline;
                }
                if chip(ui, look.boxed, "Box behind").clicked() {
                    look.boxed = !look.boxed;
                }
                if chip(ui, look.uppercase_lyrics, "CAPS lyrics").clicked() {
                    look.uppercase_lyrics = !look.uppercase_lyrics;
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Slides show as").size(12.0).color(MUTED));
                egui::ComboBox::from_id_salt("layout").width(200.0).selected_text(look.layout.name()).show_ui(ui, |ui| {
                    for l in TextLayout::ALL {
                        ui.selectable_value(&mut look.layout, l, l.name()).on_hover_text(l.help());
                    }
                });
            });
        });
        ui.add_space(6.0);
        let open = self.section_open(tab, "Effects");
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut parts = vec![format!("Dim {:.0}%", look.dim * 100.0)];
            if look.vignette {
                parts.push("soft edges".into());
            }
            if look.slow_zoom {
                parts.push("slow zoom".into());
            }
            if section_head(ui, open, "Effects", None, &parts.join(" · "), |_| {}) {
                opened = Some("Effects");
            }
            if !open {
                return;
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Dim behind text").size(12.0).color(MUTED));
                ui.add(egui::Slider::new(&mut look.dim, 0.0..=0.8).show_value(false));
            });
            ui.checkbox(&mut look.vignette, "Soft dark edges (vignette)");
            ui.checkbox(&mut look.slow_zoom, "Slow zoom on pictures");
        });
        if format!("{:?}", look) != before {
            self.sh.edit(|s| s.outputs[idx].look = look.clone());
        }
        ui.add_space(6.0);
        let open = self.section_open(tab, "Background");
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if section_head(ui, open, "Background", None, &st.outputs[idx].background.label(), |_| {}) {
                opened = Some("Background");
            }
            if !open {
                return;
            }
            ui.label(RichText::new(st.outputs[idx].background.label()).size(12.5).color(MUTED));
            ui.horizontal_wrapped(|ui| {
                for (a, b) in BG_PRESETS {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(36.0, 24.0), egui::Sense::click());
                    ui.painter().rect_filled(r, 3, rgb(a));
                    ui.painter().rect_filled(egui::Rect::from_min_max(r.center_top(), r.right_bottom()), 3, rgb(b));
                    if resp.clicked() {
                        let tr = self.transition;
                        self.sh.edit(|s| {
                            if s.mirror {
                                s.send_background(SendTarget::Both, Background::Gradient(a, b), tr);
                            } else {
                                s.outputs[idx].set_background(Background::Gradient(a, b), tr);
                            }
                        });
                    }
                }
            });
            hint(ui, "Pictures, looping videos and cameras are in the Media tab.");
        });
        ui.add_space(6.0);
        let open = self.section_open(tab, "Transition");
        let current = self.transition.name();
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            if section_head(ui, open, "Transition", None, current, |_| {}) {
                opened = Some("Transition");
            }
            if !open {
                return;
            }
            ui.horizontal_wrapped(|ui| {
                for t in Transition::ALL {
                    if chip(ui, self.transition == t, t.name()).clicked() {
                        self.transition = t;
                    }
                }
            });
            hint(ui, "Used when slides and backgrounds change.");
        });
        if let Some(name) = opened {
            self.toggle_section(tab, name);
        }
    }
}
