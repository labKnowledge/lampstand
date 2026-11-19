use crate::app::{App, LeftTab};
use crate::bible::{book_name, format_ref, parse_reference, BOOKS};
use crate::model::*;
use crate::theme::{self, *};
use eframe::egui::{self, Color32, RichText, Vec2};

pub const BG_PRESETS: [(Rgb, Rgb); 8] = [
    ([28, 32, 58], [92, 52, 74]),
    ([12, 40, 60], [20, 110, 120]),
    ([40, 20, 60], [140, 60, 110]),
    ([16, 16, 20], [52, 52, 64]),
    ([70, 40, 20], [180, 110, 50]),
    ([10, 50, 30], [60, 130, 90]),
    ([20, 30, 80], [80, 140, 220]),
    ([60, 10, 20], [160, 40, 60]),
];

impl App {
    pub fn load_chapter(&mut self, book: u8, chapter: u16, from: Option<u16>, to: Option<u16>, show: bool) {
        let tr = self.bibles.get(&self.settings.translation);
        let abbrev = tr.abbrev.clone();
        let verses: Vec<_> = tr.chapter(book, chapter).to_vec();
        if verses.is_empty() {
            self.toast(format!("{} {} isn't in this Bible", book_name(book), chapter), true);
            return;
        }
        self.bible_book = book;
        self.bible_chapter = chapter;
        let vps = self.settings.verses_per_slide.clamp(1, 4);
        let mut slides = Vec::new();
        let mut focus = None;
        if let (Some(f), Some(t)) = (from, to) {
            let range: Vec<_> = verses.iter().filter(|v| v.verse >= f && v.verse <= t).collect();
            if range.len() > 1 {
                let text = range.iter().map(|v| format!("{} {}", v.verse, v.text)).collect::<Vec<_>>().join(" ");
                let mut s = Slide::text(SlideKind::Scripture, text, format!("{} {}", format_ref(book, chapter, f, range.last().unwrap().verse), abbrev));
                s.label = format!("{}–{} together", f, range.last().unwrap().verse);
                slides.push(s);
                focus = Some(0);
            }
        }
        for chunk in verses.chunks(vps) {
            let first = chunk[0].verse;
            let last = chunk[chunk.len() - 1].verse;
            let text = if chunk.len() == 1 { chunk[0].text.clone() } else { chunk.iter().map(|v| format!("{} {}", v.verse, v.text)).collect::<Vec<_>>().join(" ") };
            let mut s = Slide::text(SlideKind::Scripture, text, format!("{} {}", format_ref(book, chapter, first, last), abbrev));
            s.label = if first == last { format!("Verse {first}") } else { format!("Verses {first}–{last}") };
            if focus.is_none() {
                if let Some(f) = from {
                    if f >= first && f <= last {
                        focus = Some(slides.len());
                    }
                }
            }
            slides.push(s);
        }
        let title = format!("{} {} · {}", book_name(book), chapter, abbrev);
        self.set_strip(title, slides, focus);
        if show {
            let i = focus.unwrap_or(0);
            self.show_strip(i);
            let r = match (from, to) {
                (Some(f), Some(t)) if t > f => format_ref(book, chapter, f, t),
                (Some(f), _) => format_ref(book, chapter, f, f),
                _ => format!("{} {}", book_name(book), chapter),
            };
            self.bible_recent.retain(|x| x != &r);
            self.bible_recent.insert(0, r);
            self.bible_recent.truncate(8);
        }
    }

    pub fn library(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (tab, label) in [(LeftTab::Bible, "Bible"), (LeftTab::Songs, "Songs & slides"), (LeftTab::Media, "Media"), (LeftTab::Breaks, "Breaks & ads")] {
                if chip(ui, self.left_tab == tab, label).clicked() {
                    self.left_tab = tab;
                }
            }
        });
        ui.add_space(8.0);
        egui::ScrollArea::vertical().id_salt("library").auto_shrink([false, false]).show(ui, |ui| match self.left_tab {
            LeftTab::Bible => self.bible_tab(ui),
            LeftTab::Songs => self.songs_tab(ui),
            LeftTab::Media => self.media_tab(ui),
            LeftTab::Breaks => self.breaks_tab(ui),
        });
    }

    fn bible_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Bible");
        ui.horizontal(|ui| {
            let cur = self.settings.translation.clone();
            let names: Vec<(String, String)> = self.bibles.translations.iter().map(|t| (t.abbrev.clone(), t.name.clone())).collect();
            egui::ComboBox::from_id_salt("translation").width(190.0).selected_text(format!("{} · {}", cur, names.iter().find(|n| n.0 == cur).map(|n| n.1.as_str()).unwrap_or(""))).show_ui(ui, |ui| {
                for (a, n) in &names {
                    if ui.selectable_label(cur == *a, format!("{a} · {n}")).clicked() {
                        self.settings.translation = a.clone();
                    }
                }
            });
            if self.settings.translation != cur {
                let (b, c) = (self.bible_book, self.bible_chapter);
                self.load_chapter(b, c, None, None, false);
            }
            ui.label(RichText::new("Verses per slide").size(12.0).color(MUTED));
            let mut vps = self.settings.verses_per_slide;
            egui::ComboBox::from_id_salt("vps").width(44.0).selected_text(vps.to_string()).show_ui(ui, |ui| {
                for n in 1..=4 {
                    ui.selectable_value(&mut vps, n, n.to_string());
                }
            });
            if vps != self.settings.verses_per_slide {
                self.settings.verses_per_slide = vps;
                let (b, c) = (self.bible_book, self.bible_chapter);
                self.load_chapter(b, c, None, None, false);
            }
        });
        ui.add_space(4.0);
        subheading(ui, "Go to a passage");
        let mut go = false;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.bible_input).hint_text("John 3:16, Ps 23, 1 Cor 13:4-7").desired_width(ui.available_width() - 70.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                go = true;
            }
            if big_button(ui, "Show", GOLD, INK, Vec2::new(60.0, 28.0)).clicked() {
                go = true;
            }
        });
        if go {
            match parse_reference(&self.bible_input) {
                Some(r) => self.load_chapter(r.book, r.chapter, r.from.or(Some(1)), r.to, true),
                None => self.toast("I couldn't read that reference. Try something like John 3:16", true),
            }
        }
        ui.horizontal(|ui| {
            let mut book = self.bible_book;
            egui::ComboBox::from_id_salt("book").width(150.0).height(400.0).selected_text(book_name(book)).show_ui(ui, |ui| {
                for (i, (name, _)) in BOOKS.iter().enumerate() {
                    ui.selectable_value(&mut book, i as u8, *name);
                }
            });
            let max_ch = self.bibles.get(&self.settings.translation).chapter_counts[book as usize].max(1);
            let mut ch = self.bible_chapter.min(max_ch);
            if book != self.bible_book {
                ch = 1;
            }
            if small_button(ui, "◀").clicked() && ch > 1 {
                ch -= 1;
            }
            ui.add(egui::DragValue::new(&mut ch).range(1..=max_ch).prefix("Ch "));
            if small_button(ui, "▶").clicked() && ch < max_ch {
                ch += 1;
            }
            if book != self.bible_book || ch != self.bible_chapter {
                self.load_chapter(book, ch, None, None, false);
            }
        });
        if !self.bible_recent.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Recent").size(12.0).color(MUTED));
                for r in self.bible_recent.clone() {
                    if chip(ui, false, &r).clicked() {
                        if let Some(p) = parse_reference(&r) {
                            self.load_chapter(p.book, p.chapter, p.from, p.to, true);
                        }
                    }
                }
            });
        }
        ui.add_space(8.0);
        subheading(ui, "Find words");
        let mut search = false;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.bible_search).hint_text("love is patient").desired_width(ui.available_width() - 76.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                search = true;
            }
            if small_button(ui, "Search").clicked() {
                search = true;
            }
        });
        if search {
            self.bible_results = self.bibles.get(&self.settings.translation).search(&self.bible_search, 150);
            if self.bible_results.is_empty() {
                self.toast("No verses matched those words", false);
            }
        }
        if !self.bible_results.is_empty() {
            hint(ui, &format!("{} verses found. Click one to show it.", self.bible_results.len()));
            let tr = self.bibles.get(&self.settings.translation);
            let rows: Vec<(u8, u16, u16, String)> = self.bible_results.iter().filter_map(|&i| tr.verses.get(i)).map(|v| (v.book, v.chapter, v.verse, v.text.clone())).collect();
            let mut pick = None;
            for (b, c, v, text) in rows {
                let r = theme::card_frame()
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(format_ref(b, c, v, v)).family(semi_family()).color(GOLD).size(12.5));
                        ui.label(RichText::new(text).size(13.0));
                    })
                    .response
                    .interact(egui::Sense::click());
                if r.clicked() {
                    pick = Some((b, c, v));
                }
                if r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
            }
            if let Some((b, c, v)) = pick {
                self.load_chapter(b, c, Some(v), None, true);
            }
        }
    }

    fn songs_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Songs & slides");
        ui.horizontal_wrapped(|ui| {
            if small_button(ui, "+ Song").clicked() {
                self.editing = Some((None, Song { title: "New song".into(), author: String::new(), lyrics: "[Verse 1]\nFirst line\nSecond line\n\n[Chorus]\nChorus line".into(), kind: SlideKind::Lyrics }));
            }
            if small_button(ui, "+ Announcement").clicked() {
                self.editing = Some((None, Song { title: "New announcement".into(), author: String::new(), lyrics: "[Event]\nEvent name\nDay, time and place".into(), kind: SlideKind::Announcement }));
            }
            if small_button(ui, "+ Titles").clicked() {
                self.editing = Some((None, Song { title: "Sermon".into(), author: String::new(), lyrics: "[Title]\nSermon title\nSpeaker name".into(), kind: SlideKind::Title }));
            }
        });
        ui.add(egui::TextEdit::singleline(&mut self.song_filter).hint_text("Filter by title or words").desired_width(f32::INFINITY));
        ui.add_space(4.0);
        let filter = self.song_filter.to_lowercase();
        let mut open = None;
        let mut edit = None;
        for (i, s) in self.lib.songs.iter().enumerate() {
            if !filter.is_empty() && !s.title.to_lowercase().contains(&filter) && !s.lyrics.to_lowercase().contains(&filter) {
                continue;
            }
            let sel = self.selected_song == Some(i);
            let tag = match s.kind {
                SlideKind::Lyrics => "SONG",
                SlideKind::Announcement => "NOTICE",
                SlideKind::Title => "TITLES",
                SlideKind::Scripture => "BIBLE",
                SlideKind::Media => "MEDIA",
            };
            let frame = egui::Frame::new()
                .fill(if sel { CARD_HI } else { CARD })
                .stroke(egui::Stroke::new(1.0, if sel { GOLD_DIM } else { LINE }))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(10, 7));
            let r = frame
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tag).family(bold_family()).size(10.5).color(GOLD));
                        ui.label(RichText::new(&s.title).size(14.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("{} slides", s.slides().len())).size(11.5).color(MUTED));
                        });
                    });
                    if !s.author.is_empty() {
                        ui.label(RichText::new(&s.author).size(12.0).color(MUTED));
                    }
                })
                .response
                .interact(egui::Sense::click());
            if r.clicked() {
                open = Some(i);
            }
            if r.double_clicked() {
                edit = Some(i);
            }
        }
        if let Some(i) = open {
            self.selected_song = Some(i);
            let s = self.lib.songs[i].clone();
            self.set_strip(s.title.clone(), s.slides(), Some(0));
        }
        if let Some(i) = edit {
            self.editing = Some((Some(i), self.lib.songs[i].clone()));
        }
        if let Some(i) = self.selected_song.filter(|i| *i < self.lib.songs.len()) {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if small_button(ui, "Edit").clicked() {
                    self.editing = Some((Some(i), self.lib.songs[i].clone()));
                }
                if small_button(ui, "Duplicate").clicked() {
                    let mut c = self.lib.songs[i].clone();
                    c.title = format!("{} (copy)", c.title);
                    self.lib.songs.push(c);
                }
            });
        }
        ui.add_space(6.0);
        hint(ui, "Tip: separate slides with a blank line. Start a slide with [Chorus] to name it.");
    }

    pub fn song_editor(&mut self, ctx: &egui::Context) {
        let Some((idx, mut song)) = self.editing.take() else { return };
        let mut keep = true;
        let mut save = false;
        let mut delete = false;
        egui::Window::new("Edit slides").collapsible(false).default_width(520.0).default_height(560.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Title");
                ui.add(egui::TextEdit::singleline(&mut song.title).desired_width(260.0));
                egui::ComboBox::from_id_salt("kind").selected_text(match song.kind {
                    SlideKind::Lyrics => "Song lyrics",
                    SlideKind::Announcement => "Announcement",
                    SlideKind::Title => "Titles",
                    _ => "Other",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut song.kind, SlideKind::Lyrics, "Song lyrics");
                    ui.selectable_value(&mut song.kind, SlideKind::Announcement, "Announcement");
                    ui.selectable_value(&mut song.kind, SlideKind::Title, "Titles");
                });
            });
            ui.horizontal(|ui| {
                ui.label("Author");
                ui.add(egui::TextEdit::singleline(&mut song.author).desired_width(260.0).hint_text("optional"));
            });
            hint(ui, "One slide per paragraph. A blank line starts a new slide. A first line like [Verse 2] names the slide.");
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut song.lyrics).desired_width(f32::INFINITY).desired_rows(16));
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if big_button(ui, "Save", GOLD, INK, Vec2::new(90.0, 32.0)).clicked() {
                    save = true;
                }
                if big_button(ui, "Cancel", CARD_HI, TEXT, Vec2::new(90.0, 32.0)).clicked() {
                    keep = false;
                }
                if idx.is_some() && big_button(ui, "Delete", Color32::from_rgb(90, 30, 34), TEXT, Vec2::new(90.0, 32.0)).clicked() {
                    delete = true;
                }
                ui.label(RichText::new(format!("{} slides", song.slides().len())).color(MUTED));
            });
        });
        if save {
            let i = match idx {
                Some(i) => {
                    self.lib.songs[i] = song.clone();
                    i
                }
                None => {
                    self.lib.songs.push(song.clone());
                    self.lib.songs.len() - 1
                }
            };
            self.selected_song = Some(i);
            self.set_strip(song.title.clone(), song.slides(), Some(0));
            self.persist();
            return;
        }
        if delete {
            if let Some(i) = idx {
                self.lib.songs.remove(i);
                self.selected_song = None;
                self.persist();
            }
            return;
        }
        if keep {
            self.editing = Some((idx, song));
        }
    }

    fn media_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Media");
        ui.horizontal(|ui| {
            if big_button(ui, "Add files…", GOLD, INK, Vec2::new(110.0, 30.0)).clicked() {
                if let Some(files) = rfd::FileDialog::new()
                    .add_filter("Pictures, videos and music", &["png", "jpg", "jpeg", "webp", "bmp", "gif", "mp4", "mov", "m4v", "mkv", "webm", "avi", "mp3", "wav", "m4a", "aac", "flac", "ogg"])
                    .pick_files()
                {
                    self.add_files(files);
                }
            }
            hint(ui, "or drag files onto the window");
        });
        ui.add_space(6.0);
        subheading(ui, "Backgrounds");
        ui.horizontal_wrapped(|ui| {
            for (a, b) in BG_PRESETS {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(46.0, 28.0), egui::Sense::click());
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(r.left_top(), rgb(a));
                mesh.colored_vertex(r.right_top(), rgb(b));
                mesh.colored_vertex(r.right_bottom(), rgb(b));
                mesh.colored_vertex(r.left_bottom(), rgb(a));
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                ui.painter().add(egui::Shape::mesh(mesh));
                if resp.hovered() {
                    ui.painter().rect_stroke(r, 2, egui::Stroke::new(2.0, GOLD), egui::StrokeKind::Outside);
                }
                if resp.on_hover_text("Use this background").clicked() {
                    self.send_background(Background::Gradient(a, b));
                }
            }
            for c in [[0, 0, 0], [255, 255, 255], [20, 60, 40]] {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(28.0, 28.0), egui::Sense::click());
                ui.painter().rect_filled(r, 2, rgb(c));
                ui.painter().rect_stroke(r, 2, egui::Stroke::new(1.0, LINE), egui::StrokeKind::Outside);
                if resp.clicked() {
                    self.send_background(Background::Color(c));
                }
            }
        });
        ui.add_space(8.0);
        subheading(ui, "Cameras & screen capture");
        hint(ui, "Cameras, HDMI capture cards and computer screens have their own panel under the monitors, with a live preview.");
        if big_button(ui, "🎥 Open Cameras & screens", CARD_HI, GOLD, Vec2::new(ui.available_width(), 32.0)).clicked() {
            self.center_tab = crate::app::CenterTab::Sources;
        }
        let media = self.lib.media.clone();
        let mut remove = None;
        for (kind, title) in [(MediaKind::Image, "Pictures"), (MediaKind::Video, "Videos"), (MediaKind::Audio, "Music & sounds")] {
            let items: Vec<(usize, &MediaFile)> = media.iter().enumerate().filter(|(_, m)| m.kind == kind).collect();
            ui.add_space(8.0);
            subheading(ui, title);
            if items.is_empty() {
                hint(ui, "Nothing yet");
                continue;
            }
            for (i, m) in items {
                theme::card_frame().inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(file_name(&m.path)).size(13.0));
                    ui.horizontal_wrapped(|ui| match kind {
                        MediaKind::Image => {
                            if small_button(ui, "Show").clicked() {
                                let s = Slide::media(MediaRef::Image(m.path.clone()));
                                self.set_strip(file_name(&m.path), vec![s], Some(0));
                                self.show_strip(0);
                            }
                            if small_button(ui, "Background").clicked() {
                                self.send_background(Background::Image(m.path.clone()));
                            }
                            if small_button(ui, "Add to break").clicked() {
                                self.sh.edit(|s| s.brk.ads.push(AdItem::Image(m.path.clone())));
                            }
                            if small_button(ui, "Logo").on_hover_text("Use as the corner logo on the stream").clicked() {
                                self.sh.edit(|s| s.logo.image = Some(m.path.clone()));
                            }
                            if small_button(ui, "✕").clicked() {
                                remove = Some(i);
                            }
                        }
                        MediaKind::Video => {
                            if small_button(ui, "Play").on_hover_text("Play full screen with sound").clicked() {
                                let s = Slide::media(MediaRef::Video { path: m.path.clone(), sound: true, looping: false });
                                self.set_strip(file_name(&m.path), vec![s], Some(0));
                                self.show_strip(0);
                            }
                            if small_button(ui, "Loop background").on_hover_text("Silent looping motion background").clicked() {
                                self.send_background(Background::Video(m.path.clone()));
                            }
                            if small_button(ui, "Add to break").clicked() {
                                self.sh.edit(|s| s.brk.ads.push(AdItem::Video(m.path.clone())));
                            }
                            if small_button(ui, "✕").clicked() {
                                remove = Some(i);
                            }
                        }
                        MediaKind::Audio => {
                            if small_button(ui, "Play now").clicked() {
                                if !self.lib.playlist.contains(&m.path) {
                                    self.lib.playlist.push(m.path.clone());
                                }
                                let idx = self.lib.playlist.iter().position(|p| p == &m.path).unwrap_or(0);
                                self.play_music(idx);
                            }
                            if small_button(ui, "Add to music list").clicked() && !self.lib.playlist.contains(&m.path) {
                                self.lib.playlist.push(m.path.clone());
                            }
                            if small_button(ui, "✕").clicked() {
                                remove = Some(i);
                            }
                        }
                    });
                });
            }
        }
        if let Some(i) = remove {
            self.lib.media.remove(i);
        }
    }

    fn breaks_tab(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Breaks & ads");
        hint(ui, "A break covers the picture with your message, a countdown and rotating ads, and can play music.");
        ui.add_space(4.0);
        let mut b = self.sh.snapshot().brk;
        let before = format!("{:?}", b);
        ui.label("Message");
        ui.add(egui::TextEdit::singleline(&mut b.message).desired_width(f32::INFINITY));
        ui.label("Second line");
        ui.add(egui::TextEdit::singleline(&mut b.sub).desired_width(f32::INFINITY));
        ui.horizontal(|ui| {
            ui.label("Countdown");
            ui.add(egui::DragValue::new(&mut b.countdown_min).range(0.0..=120.0).speed(0.5).suffix(" min"));
            ui.label("Each ad");
            ui.add(egui::DragValue::new(&mut b.ad_seconds).range(3.0..=120.0).suffix(" s"));
        });
        ui.horizontal(|ui| {
            ui.label("Show on");
            if chip(ui, b.place.screen, "Church screen").clicked() {
                b.place.screen = !b.place.screen;
            }
            if chip(ui, b.place.stream, "Stream").clicked() {
                b.place.stream = !b.place.stream;
            }
        });
        ui.checkbox(&mut b.music, "Play the music list during the break");
        ui.checkbox(&mut b.mute_mic, "Mute the microphone on the stream during the break");
        ui.checkbox(&mut b.pause_recording, "Pause the recording during the break");
        ui.add_space(6.0);
        subheading(ui, "Ads & announcements in rotation");
        let mut remove = None;
        let mut up = None;
        let n = b.ads.len();
        for (i, ad) in b.ads.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}.", i + 1)).color(MUTED));
                ui.label(RichText::new(ad.label()).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if small_button(ui, "✕").clicked() {
                        remove = Some(i);
                    }
                    if i > 0 && small_button(ui, "↑").clicked() {
                        up = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            b.ads.remove(i);
        }
        if let Some(i) = up {
            if i > 0 && i < n {
                b.ads.swap(i, i - 1);
            }
        }
        theme::card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("New text ad").family(semi_family()));
            ui.add(egui::TextEdit::singleline(&mut self.new_ad_title).hint_text("Headline, e.g. Youth camp sign-ups").desired_width(f32::INFINITY));
            ui.add(egui::TextEdit::singleline(&mut self.new_ad_body).hint_text("Details").desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.label("Colour");
                ui.color_edit_button_srgb(&mut self.new_ad_color);
                if small_button(ui, "Add ad").clicked() && !self.new_ad_title.trim().is_empty() {
                    b.ads.push(AdItem::Text { title: self.new_ad_title.trim().to_string(), body: self.new_ad_body.trim().to_string(), color: self.new_ad_color });
                    self.new_ad_title.clear();
                    self.new_ad_body.clear();
                }
                if small_button(ui, "Picture or video…").clicked() {
                    if let Some(f) = rfd::FileDialog::new().add_filter("Pictures and videos", &["png", "jpg", "jpeg", "webp", "mp4", "mov", "m4v", "webm", "mkv"]).pick_file() {
                        let p = f.to_string_lossy().to_string();
                        match media_kind_for(&p) {
                            Some(MediaKind::Image) => b.ads.push(AdItem::Image(p)),
                            Some(MediaKind::Video) => b.ads.push(AdItem::Video(p)),
                            _ => {}
                        }
                    }
                }
            });
        });
        ui.add_space(6.0);
        subheading(ui, "Break music");
        let mut rm = None;
        for (i, p) in self.lib.playlist.iter().enumerate() {
            ui.horizontal(|ui| {
                let playing = self.music_idx == Some(i) && self.sh.audio.is_playing("music");
                ui.label(RichText::new(if playing { "♪" } else { "·" }).color(GOLD));
                ui.label(RichText::new(file_name(p)).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if small_button(ui, "✕").clicked() {
                        rm = Some(i);
                    }
                });
            });
        }
        if let Some(i) = rm {
            self.lib.playlist.remove(i);
        }
        if small_button(ui, "Add music files…").clicked() {
            if let Some(files) = rfd::FileDialog::new().add_filter("Music", &["mp3", "wav", "m4a", "aac", "flac", "ogg"]).pick_files() {
                for f in files {
                    let p = f.to_string_lossy().to_string();
                    if !self.lib.playlist.contains(&p) {
                        self.lib.playlist.push(p.clone());
                    }
                    if !self.lib.media.iter().any(|m| m.path == p) {
                        self.lib.media.push(MediaFile { path: p, kind: MediaKind::Audio });
                    }
                }
            }
        }
        if format!("{:?}", b) != before {
            self.sh.edit(|s| {
                let show = s.brk.show.clone();
                let ends = s.brk.ends_at;
                s.brk = b.clone();
                s.brk.show = show;
                s.brk.ends_at = ends;
            });
        }
        ui.add_space(10.0);
        let active = self.sh.snapshot().brk.show.visible;
        if active {
            if big_button(ui, "End break", GOLD, INK, Vec2::new(ui.available_width(), 38.0)).clicked() {
                self.end_break();
            }
        } else if big_button(ui, "Start break now", CARD_HI, GOLD, Vec2::new(ui.available_width(), 38.0)).clicked() {
            self.start_break();
        }
    }
}
