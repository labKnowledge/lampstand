use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Instant;

static CLOCK: OnceLock<Instant> = OnceLock::new();

pub fn now() -> f64 {
    CLOCK.get_or_init(Instant::now).elapsed().as_secs_f64()
}

pub type Rgb = [u8; 3];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
pub enum OutputId {
    Screen,
    Stream,
}

impl OutputId {
    pub fn index(self) -> usize {
        match self {
            OutputId::Screen => 0,
            OutputId::Stream => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum SendTarget {
    Both,
    Screen,
    Stream,
}

impl SendTarget {
    pub fn includes(self, out: OutputId) -> bool {
        match self {
            SendTarget::Both => true,
            SendTarget::Screen => out == OutputId::Screen,
            SendTarget::Stream => out == OutputId::Stream,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum Background {
    Color(Rgb),
    Gradient(Rgb, Rgb),
    Image(String),
    Video(String),
    Camera(String),
}

impl Background {
    pub fn label(&self) -> String {
        match self {
            Background::Color(_) => "Solid colour".into(),
            Background::Gradient(_, _) => "Gradient".into(),
            Background::Image(p) => format!("Picture: {}", file_name(p)),
            Background::Video(p) => format!("Video: {}", file_name(p)),
            Background::Camera(c) => format!("Camera: {}", camera_label(c)),
        }
    }
}

pub fn camera_label(key: &str) -> String {
    key.split_once('|').map(|(_, n)| n.to_string()).unwrap_or_else(|| key.to_string())
}

pub fn file_name(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub enum PipLayout {
    #[default]
    Off,
    Corner,
    Side,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub struct CamAdjust {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

impl Default for CamAdjust {
    fn default() -> Self {
        CamAdjust { zoom: 1.0, x: 0.0, y: 0.0 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum SlideKind {
    Scripture,
    Lyrics,
    Announcement,
    Title,
    Media,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum MediaRef {
    Image(String),
    Video { path: String, sound: bool, looping: bool },
}

impl MediaRef {
    pub fn path(&self) -> &str {
        match self {
            MediaRef::Image(p) => p,
            MediaRef::Video { path, .. } => path,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Slide {
    pub kind: SlideKind,
    pub text: String,
    pub caption: String,
    pub media: Option<MediaRef>,
    pub label: String,
}

impl Slide {
    pub fn text(kind: SlideKind, text: impl Into<String>, caption: impl Into<String>) -> Self {
        Slide { kind, text: text.into(), caption: caption.into(), media: None, label: String::new() }
    }
    pub fn media(m: MediaRef) -> Self {
        let label = file_name(m.path());
        Slide { kind: SlideKind::Media, text: String::new(), caption: String::new(), media: Some(m), label }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Transition {
    Cut,
    Fade,
    Push,
    Rise,
    Wipe,
}

impl Transition {
    pub const ALL: [Transition; 5] = [Transition::Cut, Transition::Fade, Transition::Push, Transition::Rise, Transition::Wipe];
    pub fn name(self) -> &'static str {
        match self {
            Transition::Cut => "Cut",
            Transition::Fade => "Fade",
            Transition::Push => "Push",
            Transition::Rise => "Rise",
            Transition::Wipe => "Wipe",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum FontChoice {
    Serif,
    Sans,
    SansBold,
}

impl FontChoice {
    pub const ALL: [FontChoice; 3] = [FontChoice::Serif, FontChoice::Sans, FontChoice::SansBold];
    pub fn name(self) -> &'static str {
        match self {
            FontChoice::Serif => "Playfair (serif)",
            FontChoice::Sans => "Figtree",
            FontChoice::SansBold => "Figtree bold",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum TextLayout {
    Full,
    LowerThird,
    Cover,
    Transparent,
    SideLeft,
    SideRight,
}

impl TextLayout {
    pub const ALL: [TextLayout; 6] = [TextLayout::Cover, TextLayout::Full, TextLayout::Transparent, TextLayout::LowerThird, TextLayout::SideLeft, TextLayout::SideRight];
    pub fn name(self) -> &'static str {
        match self {
            TextLayout::Cover => "Full cover",
            TextLayout::Full => "Over picture",
            TextLayout::Transparent => "See-through",
            TextLayout::LowerThird => "Lower third",
            TextLayout::SideLeft => "Slide left · picture right",
            TextLayout::SideRight => "Picture left · slide right",
        }
    }
    pub fn help(self) -> &'static str {
        match self {
            TextLayout::Cover => "Text on its own background; the camera is hidden",
            TextLayout::Full => "Full-screen text over a darkened camera or background",
            TextLayout::Transparent => "Full-screen text straight over the camera, nothing darkened",
            TextLayout::LowerThird => "Text in a bar along the bottom; the camera stays visible",
            TextLayout::SideLeft => "Slide on the left half, camera or screen on the right",
            TextLayout::SideRight => "Camera or screen on the left, slide on the right half",
        }
    }
    pub fn is_side(self) -> bool {
        matches!(self, TextLayout::SideLeft | TextLayout::SideRight)
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct Look {
    pub font: FontChoice,
    pub color: Rgb,
    pub size: f32,
    pub shadow: bool,
    pub outline: bool,
    pub boxed: bool,
    pub uppercase_lyrics: bool,
    pub layout: TextLayout,
    pub dim: f32,
    pub vignette: bool,
    pub slow_zoom: bool,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            font: FontChoice::Serif,
            color: [255, 255, 255],
            size: 1.0,
            shadow: true,
            outline: false,
            boxed: false,
            uppercase_lyrics: false,
            layout: TextLayout::Full,
            dim: 0.25,
            vignette: true,
            slow_zoom: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OutputLive {
    pub background: Background,
    pub prev_background: Option<Background>,
    pub bg_t0: f64,
    pub slide: Option<Slide>,
    pub prev_slide: Option<Slide>,
    pub slide_t0: f64,
    pub transition: Transition,
    pub look: Look,
    pub black: bool,
    pub black_t0: f64,
    #[serde(default)]
    pub pip: Option<Background>,
    #[serde(default)]
    pub pip_layout: PipLayout,
    #[serde(default)]
    pub pip_t0: f64,
    #[serde(default)]
    pub clean: bool,
    #[serde(default)]
    pub clean_t0: f64,
    #[serde(default = "default_pip_size")]
    pub pip_size: f32,
    #[serde(default = "default_pip_corner")]
    pub pip_corner: u8,
}

fn default_pip_size() -> f32 {
    0.36
}

fn default_pip_corner() -> u8 {
    1
}

impl OutputLive {
    pub fn new(background: Background, look: Look) -> Self {
        OutputLive {
            background,
            prev_background: None,
            bg_t0: -10.0,
            slide: None,
            prev_slide: None,
            slide_t0: -10.0,
            transition: Transition::Fade,
            look,
            black: false,
            black_t0: -10.0,
            pip: None,
            pip_layout: PipLayout::Off,
            pip_t0: -10.0,
            clean: false,
            clean_t0: -10.0,
            pip_size: 0.36,
            pip_corner: 1,
        }
    }
    pub fn set_pip(&mut self, pip: Option<Background>, layout: PipLayout) {
        let layout = if pip.is_none() { PipLayout::Off } else { layout };
        if self.pip != pip || self.pip_layout != layout {
            self.pip = pip;
            self.pip_layout = layout;
            self.pip_t0 = now();
        }
    }
    pub fn set_clean(&mut self, clean: bool) {
        if self.clean != clean {
            self.clean = clean;
            self.clean_t0 = now();
        }
    }
    pub fn set_slide(&mut self, slide: Option<Slide>, transition: Transition) {
        if self.slide == slide {
            return;
        }
        self.prev_slide = self.slide.take();
        self.slide = slide;
        self.slide_t0 = now();
        self.transition = transition;
    }
    pub fn set_background(&mut self, bg: Background, transition: Transition) {
        if self.background == bg {
            return;
        }
        self.prev_background = Some(std::mem::replace(&mut self.background, bg));
        self.bg_t0 = now();
        self.transition = transition;
    }
    pub fn set_black(&mut self, black: bool) {
        if self.black != black {
            self.black = black;
            self.black_t0 = now();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum LtStyle {
    Classic,
    Clean,
    Banner,
    Minimal,
}

impl LtStyle {
    pub const ALL: [LtStyle; 4] = [LtStyle::Classic, LtStyle::Clean, LtStyle::Banner, LtStyle::Minimal];
    pub fn name(self) -> &'static str {
        match self {
            LtStyle::Classic => "Classic bar",
            LtStyle::Clean => "Clean card",
            LtStyle::Banner => "Full banner",
            LtStyle::Minimal => "Minimal line",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Anim {
    Slide,
    Fade,
    Wipe,
    Rise,
}

impl Anim {
    pub const ALL: [Anim; 4] = [Anim::Slide, Anim::Fade, Anim::Wipe, Anim::Rise];
    pub fn name(self) -> &'static str {
        match self {
            Anim::Slide => "Slide in",
            Anim::Fade => "Fade",
            Anim::Wipe => "Wipe",
            Anim::Rise => "Rise up",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
pub struct Where {
    pub screen: bool,
    pub stream: bool,
}

impl Where {
    pub fn on(self, out: OutputId) -> bool {
        match out {
            OutputId::Screen => self.screen,
            OutputId::Stream => self.stream,
        }
    }
    pub fn stream_only() -> Self {
        Where { screen: false, stream: true }
    }
    pub fn both() -> Self {
        Where { screen: true, stream: true }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Showable {
    pub visible: bool,
    pub t0: f64,
}

impl Default for Showable {
    fn default() -> Self {
        Showable { visible: false, t0: -10.0 }
    }
}

impl Showable {
    pub fn set(&mut self, v: bool) {
        if self.visible != v {
            self.visible = v;
            self.t0 = now();
        }
    }
    pub fn amount(&self, t: f64, dur: f64) -> f32 {
        let p = ((t - self.t0) / dur).clamp(0.0, 1.0) as f32;
        if self.visible { p } else { 1.0 - p }
    }
    pub fn active(&self, t: f64, dur: f64) -> bool {
        self.visible || t - self.t0 < dur
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LowerThird {
    pub name: String,
    pub title: String,
    pub style: LtStyle,
    pub anim: Anim,
    pub accent: Rgb,
    pub show: Showable,
    pub place: Where,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TitleCard {
    pub text: String,
    pub sub: String,
    pub centered: bool,
    pub anim: Anim,
    pub show: Showable,
    pub place: Where,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Caption {
    pub lines: Vec<String>,
    pub show: Showable,
    pub place: Where,
    pub last_t: f64,
    pub auto_clear: f32,
    pub large: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Ticker {
    pub text: String,
    pub speed: f32,
    pub show: Showable,
    pub place: Where,
    pub color: Rgb,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LogoBug {
    pub text: String,
    pub image: Option<String>,
    pub show: Showable,
    pub place: Where,
    pub live_badge: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub enum AdItem {
    Text { title: String, body: String, color: Rgb },
    Image(String),
    Video(String),
}

impl AdItem {
    pub fn label(&self) -> String {
        match self {
            AdItem::Text { title, .. } => title.clone(),
            AdItem::Image(p) => format!("Picture: {}", file_name(p)),
            AdItem::Video(p) => format!("Video: {}", file_name(p)),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BreakScene {
    pub message: String,
    pub sub: String,
    pub countdown_min: f32,
    pub ad_seconds: f32,
    pub ads: Vec<AdItem>,
    pub place: Where,
    pub music: bool,
    pub mute_mic: bool,
    pub pause_recording: bool,
    pub show: Showable,
    pub ends_at: Option<f64>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LiveState {
    pub outputs: [OutputLive; 2],
    pub mirror: bool,
    pub lower_third: LowerThird,
    pub title: TitleCard,
    pub caption: Caption,
    pub ticker: Ticker,
    pub logo: LogoBug,
    pub brk: BreakScene,
    pub is_live: bool,
    pub canvas: (u32, u32),
    pub version: u64,
    #[serde(default)]
    pub cam_adjust: std::collections::HashMap<String, CamAdjust>,
}

pub const WARM_BG: (Rgb, Rgb) = ([28, 32, 58], [92, 52, 74]);

impl Default for LiveState {
    fn default() -> Self {
        let bg = Background::Gradient(WARM_BG.0, WARM_BG.1);
        let screen = OutputLive::new(bg.clone(), Look::default());
        let stream_look = Look { layout: TextLayout::LowerThird, font: FontChoice::Sans, ..Look::default() };
        let stream = OutputLive::new(bg, stream_look);
        LiveState {
            outputs: [screen, stream],
            mirror: true,
            lower_third: LowerThird {
                name: "Pastor Sam Adeyemi".into(),
                title: "Lead Pastor".into(),
                style: LtStyle::Classic,
                anim: Anim::Slide,
                accent: [224, 178, 82],
                show: Showable::default(),
                place: Where::stream_only(),
            },
            title: TitleCard {
                text: "Sunday Service".into(),
                sub: "Walking in faith".into(),
                centered: false,
                anim: Anim::Rise,
                show: Showable::default(),
                place: Where::stream_only(),
            },
            caption: Caption {
                lines: vec![],
                show: Showable::default(),
                place: Where::both(),
                last_t: 0.0,
                auto_clear: 8.0,
                large: false,
            },
            ticker: Ticker {
                text: "Welcome to our live service  •  Give online at the church website  •  Prayer requests: send a message to the prayer team".into(),
                speed: 90.0,
                show: Showable::default(),
                place: Where::stream_only(),
                color: [224, 178, 82],
            },
            logo: LogoBug {
                text: "Grace Community Church".into(),
                image: None,
                show: Showable::default(),
                place: Where::stream_only(),
                live_badge: true,
            },
            brk: BreakScene {
                message: "We'll be right back".into(),
                sub: "Stay with us. The service continues shortly.".into(),
                countdown_min: 5.0,
                ad_seconds: 8.0,
                ads: vec![
                    AdItem::Text {
                        title: "Youth night this Friday".into(),
                        body: "7 pm in the main hall. Bring a friend!".into(),
                        color: [60, 90, 160],
                    },
                    AdItem::Text {
                        title: "Give online".into(),
                        body: "Support the ministry from anywhere, any time.".into(),
                        color: [120, 70, 110],
                    },
                ],
                place: Where::both(),
                music: true,
                mute_mic: true,
                pause_recording: false,
                show: Showable::default(),
                ends_at: None,
            },
            is_live: false,
            canvas: (1280, 720),
            version: 1,
            cam_adjust: std::collections::HashMap::new(),
        }
    }
}

impl LiveState {
    pub fn out(&self, o: OutputId) -> &OutputLive {
        &self.outputs[o.index()]
    }
    pub fn out_mut(&mut self, o: OutputId) -> &mut OutputLive {
        &mut self.outputs[o.index()]
    }
    pub fn touch(&mut self) {
        self.version = self.version.wrapping_add(1);
    }
    pub fn send_slide(&mut self, target: SendTarget, slide: Option<Slide>, tr: Transition) {
        for o in [OutputId::Screen, OutputId::Stream] {
            if self.mirror || target.includes(o) {
                self.out_mut(o).set_slide(slide.clone(), tr);
            }
        }
        self.touch();
    }
    pub fn send_background(&mut self, target: SendTarget, bg: Background, tr: Transition) {
        for o in [OutputId::Screen, OutputId::Stream] {
            if self.mirror || target.includes(o) {
                self.out_mut(o).set_background(bg.clone(), tr);
            }
        }
        self.touch();
    }
    pub fn send_pip(&mut self, target: SendTarget, pip: Option<Background>, layout: PipLayout) {
        for o in [OutputId::Screen, OutputId::Stream] {
            if self.mirror || target.includes(o) {
                self.out_mut(o).set_pip(pip.clone(), layout);
            }
        }
        self.touch();
    }
    pub fn set_mirror(&mut self, mirror: bool) {
        self.mirror = mirror;
        if mirror {
            let s = self.outputs[0].clone();
            let t = &mut self.outputs[1];
            t.set_background(s.background.clone(), Transition::Fade);
            t.set_slide(s.slide.clone(), Transition::Fade);
            t.set_black(s.black);
            t.set_pip(s.pip.clone(), s.pip_layout);
            t.set_clean(s.clean);
        }
        self.touch();
    }
    pub fn look_for(&self, o: OutputId) -> &Look {
        if self.mirror { &self.outputs[0].look } else { &self.out(o).look }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Song {
    pub title: String,
    pub author: String,
    pub lyrics: String,
    pub kind: SlideKind,
}

impl Song {
    pub fn slides(&self) -> Vec<Slide> {
        let mut out = Vec::new();
        for block in self.lyrics.split("\n\n") {
            let block = block.trim();
            if block.is_empty() {
                continue;
            }
            let mut lines: Vec<&str> = block.lines().collect();
            let mut label = String::new();
            if let Some(first) = lines.first() {
                let f = first.trim();
                if f.starts_with('[') && f.ends_with(']') {
                    label = f.trim_matches(|c| c == '[' || c == ']').to_string();
                    lines.remove(0);
                }
            }
            let text = lines.join("\n");
            let caption = if self.kind == SlideKind::Lyrics {
                if self.author.is_empty() { self.title.clone() } else { format!("{} · {}", self.title, self.author) }
            } else {
                String::new()
            };
            let mut s = Slide::text(self.kind, text, caption);
            s.label = label;
            out.push(s);
        }
        out
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Person {
    pub name: String,
    pub title: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct MediaFile {
    pub path: String,
    pub kind: MediaKind,
}

pub fn media_kind_for(path: &str) -> Option<MediaKind> {
    let ext = std::path::Path::new(path).extension()?.to_string_lossy().to_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" => Some(MediaKind::Image),
        "mp4" | "mov" | "m4v" | "mkv" | "webm" | "avi" | "wmv" | "mpg" | "mpeg" => Some(MediaKind::Video),
        "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg" | "opus" | "wma" => Some(MediaKind::Audio),
        _ => None,
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Destination {
    pub service: String,
    pub name: String,
    pub server: String,
    pub key: String,
    pub enabled: bool,
}

impl Destination {
    pub fn url(&self) -> String {
        let server = self.server.trim();
        let key = self.key.trim();
        let server = server.trim_end_matches(' ');
        if key.is_empty() {
            return server.to_string();
        }
        if server.ends_with('/') { format!("{server}{key}") } else { format!("{server}/{key}") }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Quality {
    Hd720,
    Hd1080,
}

impl Quality {
    pub fn size(self) -> (u32, u32) {
        match self {
            Quality::Hd720 => (1280, 720),
            Quality::Hd1080 => (1920, 1080),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Quality::Hd720 => "720p (good for most church internet)",
            Quality::Hd1080 => "1080p (needs fast upload)",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Settings {
    pub church_name: String,
    pub destinations: Vec<Destination>,
    pub quality: Quality,
    pub bitrate_kbps: u32,
    pub encoder: String,
    pub record_dir: String,
    pub ffmpeg_path: String,
    pub mic_device: String,
    pub output_device: String,
    pub record_with_live: bool,
    pub monitor_in_room: bool,
    pub projector_display: usize,
    pub verses_per_slide: usize,
    pub translation: String,
}

impl Default for Settings {
    fn default() -> Self {
        let record_dir = dirs::video_dir()
            .or_else(dirs::home_dir)
            .map(|p| p.join("Lampstand recordings").to_string_lossy().to_string())
            .unwrap_or_else(|| "recordings".into());
        Settings {
            church_name: "Grace Community Church".into(),
            destinations: vec![],
            quality: Quality::Hd720,
            bitrate_kbps: 4000,
            encoder: "libx264".into(),
            record_dir,
            ffmpeg_path: String::new(),
            mic_device: String::new(),
            output_device: String::new(),
            record_with_live: true,
            monitor_in_room: true,
            projector_display: 2,
            verses_per_slide: 1,
            translation: "WEB".into(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Library {
    pub songs: Vec<Song>,
    pub people: Vec<Person>,
    pub media: Vec<MediaFile>,
    pub playlist: Vec<String>,
    pub caption_script: String,
}

impl Library {
    pub fn sample() -> Self {
        Library {
            songs: vec![
                Song {
                    title: "Amazing Grace".into(),
                    author: "John Newton".into(),
                    kind: SlideKind::Lyrics,
                    lyrics: "[Verse 1]\nAmazing grace! How sweet the sound\nThat saved a wretch like me!\nI once was lost, but now am found;\nWas blind, but now I see.\n\n[Verse 2]\n'Twas grace that taught my heart to fear,\nAnd grace my fears relieved;\nHow precious did that grace appear\nThe hour I first believed.\n\n[Verse 3]\nThrough many dangers, toils and snares,\nI have already come;\n'Tis grace hath brought me safe thus far,\nAnd grace will lead me home.\n\n[Verse 4]\nWhen we've been there ten thousand years,\nBright shining as the sun,\nWe've no less days to sing God's praise\nThan when we'd first begun.".into(),
                },
                Song {
                    title: "Holy, Holy, Holy".into(),
                    author: "Reginald Heber".into(),
                    kind: SlideKind::Lyrics,
                    lyrics: "[Verse 1]\nHoly, holy, holy! Lord God Almighty!\nEarly in the morning our song shall rise to Thee;\nHoly, holy, holy, merciful and mighty!\nGod in three Persons, blessed Trinity!\n\n[Verse 2]\nHoly, holy, holy! All the saints adore Thee,\nCasting down their golden crowns around the glassy sea;\nCherubim and seraphim falling down before Thee,\nWho wert, and art, and evermore shalt be.".into(),
                },
                Song {
                    title: "This week at church".into(),
                    author: String::new(),
                    kind: SlideKind::Announcement,
                    lyrics: "[Welcome]\nWelcome home!\nWe're glad you're here today.\n\n[Prayer]\nPrayer meeting\nWednesday, 6:30 pm in the chapel\n\n[Giving]\nGive online\nScan the code at the welcome desk\n\n[Next Sunday]\nBaptism Sunday\nSign up at the info desk".into(),
                },
                Song {
                    title: "Sermon titles".into(),
                    author: String::new(),
                    kind: SlideKind::Title,
                    lyrics: "[Series]\nWalking in Faith\nPart 3: Trust in the storm\n\n[Point 1]\nFaith listens\n\n[Point 2]\nFaith steps out\n\n[Point 3]\nFaith rests".into(),
                },
            ],
            people: vec![
                Person { name: "Pastor Sam Adeyemi".into(), title: "Lead Pastor".into() },
                Person { name: "Grace Mensah".into(), title: "Worship Leader".into() },
                Person { name: "Daniel Okafor".into(), title: "Announcements".into() },
            ],
            media: vec![],
            playlist: vec![],
            caption_script: "Good morning and welcome to our service.\nPlease open your Bibles to John chapter 3.\nLet us pray together.".into(),
        }
    }
}
