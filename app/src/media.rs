use crate::audio::{AudioEngine, SrcKind};
use crate::model::now;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub fn video_key(path: &str, looping: bool, sound: bool) -> String {
    format!("v:{}{}:{}", looping as u8, sound as u8, path)
}

pub fn camera_key(spec: &str) -> String {
    format!("c:{spec}")
}

pub fn key_has_sound(key: &str) -> bool {
    key.starts_with("v:") && key.as_bytes().get(3) == Some(&b'1')
}

pub fn ffmpeg_command(path: &Path) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

fn candidates() -> Vec<PathBuf> {
    let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let mut v = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(d) = cur.parent() {
            v.push(d.join(exe));
            v.push(d.join("ffmpeg").join(exe));
            v.push(d.join("../Resources").join(exe));
        }
    }
    if let Some(d) = dirs::data_dir() {
        v.push(d.join("Lampstand").join("ffmpeg").join(exe));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            v.push(d.join(exe));
        }
    }
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/snap/bin", "C:\\ffmpeg\\bin", "C:\\Program Files\\ffmpeg\\bin"] {
        v.push(PathBuf::from(d).join(exe));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        v.push(PathBuf::from(local).join("Microsoft").join("WinGet").join("Links").join(exe));
    }
    v
}

pub struct FfmpegInfo {
    pub path: PathBuf,
    pub version: String,
    pub encoders: Vec<String>,
}

pub fn locate_ffmpeg(custom: &str) -> Option<FfmpegInfo> {
    let mut list = Vec::new();
    if !custom.trim().is_empty() {
        list.push(PathBuf::from(custom.trim()));
    }
    list.extend(candidates());
    for p in list {
        if !p.is_file() {
            continue;
        }
        let Ok(out) = ffmpeg_command(&p).arg("-hide_banner").arg("-version").stdin(Stdio::null()).output() else { continue };
        if !out.status.success() {
            continue;
        }
        let version = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("").to_string();
        let mut encoders = Vec::new();
        if let Ok(enc) = ffmpeg_command(&p).args(["-hide_banner", "-encoders"]).stdin(Stdio::null()).output() {
            let s = String::from_utf8_lossy(&enc.stdout);
            for name in ["libx264", "h264_videotoolbox", "h264_nvenc", "h264_qsv", "h264_amf", "h264_vaapi"] {
                if s.lines().any(|l| l.split_whitespace().nth(1) == Some(name)) {
                    encoders.push(name.to_string());
                }
            }
        }
        return Some(FfmpegInfo { path: p, version, encoders });
    }
    None
}

#[derive(Clone, Debug, PartialEq)]
pub struct CameraDevice {
    pub spec: String,
    pub name: String,
    pub screen: bool,
}

static MAC_DEVICES: Mutex<Option<(std::time::Instant, HashMap<String, String>)>> = Mutex::new(None);

pub fn mac_device_index(name: &str) -> Option<String> {
    MAC_DEVICES.lock().unwrap().as_ref().and_then(|(_, m)| m.get(name).cloned())
}

pub fn refresh_mac_devices(ffmpeg: &Path) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let fresh = MAC_DEVICES.lock().unwrap().as_ref().map(|(t, _)| t.elapsed().as_secs_f32() < 3.0).unwrap_or(false);
    if !fresh {
        list_cameras(ffmpeg);
    }
}

pub fn list_cameras(ffmpeg: &Path) -> Vec<CameraDevice> {
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        let mut names: HashMap<String, String> = HashMap::new();
        let Ok(o) = ffmpeg_command(ffmpeg).args(["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""]).stdin(Stdio::null()).output() else { return out };
        let s = String::from_utf8_lossy(&o.stderr);
        let mut in_video = false;
        for line in s.lines() {
            if line.contains("video devices") {
                in_video = true;
                continue;
            }
            if line.contains("audio devices") {
                break;
            }
            if in_video {
                if let Some(rest) = line.split("] [").nth(1) {
                    if let Some((idx, name)) = rest.split_once("] ") {
                        let name = name.trim();
                        names.insert(name.to_string(), idx.trim().to_string());
                        if let Some(n) = name.strip_prefix("Capture screen ") {
                            let shown = n.trim().parse::<usize>().map(|i| format!("Screen {}", i + 1)).unwrap_or_else(|_| name.to_string());
                            let ord = n.trim().parse::<usize>().unwrap_or(0);
                            out.push(CameraDevice { spec: format!("screen:@{ord}|{shown}"), name: shown, screen: true });
                        } else {
                            out.push(CameraDevice { spec: format!("{}|{}", idx.trim(), name), name: name.to_string(), screen: false });
                        }
                    }
                }
            }
        }
        *MAC_DEVICES.lock().unwrap() = Some((std::time::Instant::now(), names));
    } else if cfg!(windows) {
        let Ok(o) = ffmpeg_command(ffmpeg).args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"]).stdin(Stdio::null()).output() else { return out };
        let s = String::from_utf8_lossy(&o.stderr);
        for line in s.lines() {
            if line.contains("(video)") {
                if let Some(start) = line.find('"') {
                    if let Some(end) = line[start + 1..].find('"') {
                        let name = &line[start + 1..start + 1 + end];
                        out.push(CameraDevice { spec: format!("{name}|{name}"), name: name.to_string(), screen: false });
                    }
                }
            }
        }
    } else if let Ok(rd) = std::fs::read_dir("/dev") {
        let mut devs: Vec<String> = rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("video"))
            .collect();
        devs.sort();
        for d in devs {
            let name = std::fs::read_to_string(format!("/sys/class/video4linux/{d}/name")).map(|s| s.trim().to_string()).unwrap_or_else(|_| d.clone());
            out.push(CameraDevice { spec: format!("/dev/{d}|{name}"), name, screen: false });
        }
    }
    if cfg!(windows) {
        out.push(CameraDevice { spec: "screen:desktop|Whole desktop".into(), name: "Whole desktop".into(), screen: true });
    } else if !cfg!(target_os = "macos") {
        let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
        out.push(CameraDevice { spec: format!("screen:{display}|Screen {display}"), name: format!("Screen {display}"), screen: true });
    }
    out
}

pub const PERMISSION_MSG: &str = "macOS blocked screen recording for Lampstand: open Cameras & screens and follow the three steps";

const GENERIC_MSG: &str = "Couldn't open this screen or camera";

pub fn is_generic_error(line: &str) -> bool {
    line == GENERIC_MSG
}

pub fn useful_error(line: &str) -> Option<String> {
    let l = line.trim();
    if l.is_empty() || l.contains("NSKVONotifying") || l.starts_with("objc[") || l.contains("falling back to default") {
        return None;
    }
    if l.contains("Invalid device index") || l.contains("Video device not found") {
        return Some("That screen or camera isn't connected any more. Press Find cameras & screens and pick it again".into());
    }
    let lower = l.to_lowercase();
    if lower.contains("not authorized") || lower.contains("denied") || lower.contains("failed to create av capture input") || lower.contains("permission") {
        return Some(PERMISSION_MSG.into());
    }
    if l.contains("Error opening input") || l.starts_with("Error opening input file") {
        return Some(GENERIC_MSG.into());
    }
    let body = l.rsplit("] ").next().unwrap_or(l).trim();
    if !body.contains(' ') && body.len() <= 12 {
        return None;
    }
    if body.starts_with("Supported") || body.contains("Supported pixel formats") || body.contains("Supported modes") {
        return None;
    }
    if body.contains("pixel format") && body.contains("not supported") {
        return Some("The device doesn't offer the requested picture format; trying another one".into());
    }
    if body.contains("framerate") && body.contains("not supported") {
        return Some("The device doesn't offer 30 frames per second; trying its own setting".into());
    }
    Some(body.to_string())
}

pub struct VideoFrame {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

struct VideoSource {
    latest: Arc<Mutex<Option<Arc<VideoFrame>>>>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    last_wanted: f64,
    started: f64,
    size: (u32, u32),
    sound_key: Option<String>,
}

impl VideoSource {
    fn kill(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

pub struct VideoHub {
    pub ffmpeg: Arc<RwLock<Option<PathBuf>>>,
    sources: Mutex<HashMap<String, VideoSource>>,
    pub errors: Arc<Mutex<HashMap<String, String>>>,
    closing: AtomicBool,
    audio: Arc<AudioEngine>,
    derived: Mutex<HashMap<String, (usize, Arc<VideoFrame>)>>,
    pub focus: Mutex<Option<String>>,
    active_screen: Mutex<Option<String>>,
    on_air_screen: Mutex<Option<String>>,
}

pub fn mac_window_base(key: &str) -> Option<(String, [f32; 6])> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let spec = key.strip_prefix("c:window:@")?;
    let body = spec.split('|').next()?;
    let mut it = body.split(',');
    let ord: usize = it.next()?.trim().parse().ok()?;
    let v: Vec<f32> = it.filter_map(|x| x.trim().parse().ok()).collect();
    if v.len() < 6 {
        return None;
    }
    Some((camera_key(&format!("screen:@{ord}|Screen {}", ord + 1)), [v[0], v[1], v[2], v[3], v[4], v[5]]))
}

pub fn is_screen_key(key: &str) -> bool {
    key.starts_with("c:screen:") || key.starts_with("c:window:")
}

fn crop_frame(base: &VideoFrame, c: [f32; 6], out_w: u32, out_h: u32) -> VideoFrame {
    let (bw, bh) = (base.w as f32, base.h as f32);
    let (sw, sh) = (c[4].max(1.0), c[5].max(1.0));
    let s = (bw / sw).min(bh / sh);
    let (ox, oy) = ((bw - sw * s) / 2.0, (bh - sh * s) / 2.0);
    let cx = (ox + c[0] * s).clamp(0.0, bw - 2.0);
    let cy = (oy + c[1] * s).clamp(0.0, bh - 2.0);
    let cw = (c[2] * s).clamp(2.0, bw - cx);
    let ch = (c[3] * s).clamp(2.0, bh - cy);
    let fit = (out_w as f32 / cw).min(out_h as f32 / ch);
    let (dw, dh) = (cw * fit, ch * fit);
    let (dx, dy) = ((out_w as f32 - dw) / 2.0, (out_h as f32 - dh) / 2.0);
    let mut data = vec![0u8; (out_w * out_h * 4) as usize];
    for p in data.chunks_exact_mut(4) {
        p[3] = 255;
    }
    let stride = base.w as usize * 4;
    let y0 = dy.max(0.0) as u32;
    let y1 = ((dy + dh) as u32).min(out_h);
    let x0 = dx.max(0.0) as u32;
    let x1 = ((dx + dw) as u32).min(out_w);
    for y in y0..y1 {
        let sy = (cy + (y as f32 + 0.5 - dy) / fit - 0.5).clamp(0.0, bh - 2.0);
        let iy = sy as usize;
        let ty = sy - iy as f32;
        for x in x0..x1 {
            let sx = (cx + (x as f32 + 0.5 - dx) / fit - 0.5).clamp(0.0, bw - 2.0);
            let ix = sx as usize;
            let tx = sx - ix as f32;
            let i00 = iy * stride + ix * 4;
            let i01 = i00 + stride;
            let o = ((y * out_w + x) * 4) as usize;
            for k in 0..3 {
                let top = base.data[i00 + k] as f32 * (1.0 - tx) + base.data[i00 + 4 + k] as f32 * tx;
                let bot = base.data[i01 + k] as f32 * (1.0 - tx) + base.data[i01 + 4 + k] as f32 * tx;
                data[o + k] = (top * (1.0 - ty) + bot * ty) as u8;
            }
        }
    }
    VideoFrame { w: out_w, h: out_h, data }
}

impl VideoHub {
    pub fn new(ffmpeg: Arc<RwLock<Option<PathBuf>>>, audio: Arc<AudioEngine>) -> Self {
        VideoHub { ffmpeg, sources: Mutex::new(HashMap::new()), errors: Arc::new(Mutex::new(HashMap::new())), closing: AtomicBool::new(false), audio, derived: Mutex::new(HashMap::new()), focus: Mutex::new(None), active_screen: Mutex::new(None), on_air_screen: Mutex::new(None) }
    }

    pub fn frame(&self, key: &str) -> Option<Arc<VideoFrame>> {
        if let Some((base, crop)) = mac_window_base(key) {
            let bf = self.frame(&base)?;
            let stamp = Arc::as_ptr(&bf) as usize;
            let mut d = self.derived.lock().unwrap();
            if let Some((st, f)) = d.get(key) {
                if *st == stamp {
                    return Some(f.clone());
                }
            }
            let f = Arc::new(crop_frame(&bf, crop, 1280, 720));
            d.insert(key.to_string(), (stamp, f.clone()));
            return Some(f);
        }
        let s = self.sources.lock().unwrap();
        s.get(key).and_then(|v| v.latest.lock().unwrap().clone())
    }

    fn base_key(key: &str) -> String {
        mac_window_base(key).map(|(b, _)| b).unwrap_or_else(|| key.to_string())
    }

    pub fn error_for(&self, key: &str) -> Option<String> {
        let e = self.errors.lock().unwrap();
        e.get(key).or_else(|| e.get(&Self::base_key(key))).cloned()
    }

    pub fn single_screen_rule() -> bool {
        cfg!(target_os = "macos")
    }

    pub fn active_screen(&self) -> Option<String> {
        self.active_screen.lock().unwrap().clone()
    }

    pub fn is_live_capture(&self, key: &str) -> bool {
        !Self::single_screen_rule() || !is_screen_key(key) || self.active_screen().as_deref() == Some(Self::base_key(key).as_str())
    }

    pub fn screen_on_air_elsewhere(&self, key: &str) -> bool {
        Self::single_screen_rule() && is_screen_key(key) && self.active_screen().map(|a| a != Self::base_key(key)).unwrap_or(false) && self.on_air_screen.lock().unwrap().is_some()
    }

    pub fn ensure_prioritized(&self, wanted_in: &HashMap<String, bool>, on_air: &[String], size: (u32, u32)) {
        let t = now();
        let mut wanted: HashMap<String, bool> = HashMap::new();
        for (k, v) in wanted_in {
            let e = wanted.entry(Self::base_key(k)).or_insert(false);
            *e |= *v;
        }
        if Self::single_screen_rule() {
            let mut air: Vec<String> = on_air.iter().filter(|k| is_screen_key(k)).map(|k| Self::base_key(k)).collect();
            air.sort();
            let focus = self.focus.lock().unwrap().clone().map(|k| Self::base_key(&k)).filter(|k| is_screen_key(k) && wanted.contains_key(k));
            let mut rest: Vec<String> = wanted.keys().filter(|k| is_screen_key(k)).cloned().collect();
            rest.sort();
            let chosen = air.first().cloned().or(focus).or_else(|| rest.first().cloned());
            *self.on_air_screen.lock().unwrap() = air.first().cloned();
            wanted.retain(|k, _| !is_screen_key(k) || Some(k) == chosen.as_ref());
            let mut sources = self.sources.lock().unwrap();
            let others: Vec<String> = sources.keys().filter(|k| is_screen_key(k) && Some(*k) != chosen.as_ref()).cloned().collect();
            for k in others {
                if let Some(s) = sources.remove(&k) {
                    s.kill();
                }
            }
            *self.active_screen.lock().unwrap() = chosen;
        }
        let wanted = &wanted;
        let mut sources = self.sources.lock().unwrap();
        for (k, _) in wanted.iter() {
            let size = if is_screen_key(k) { (size.0.max(1920), size.1.max(1080)) } else { size };
            if let Some(s) = sources.get_mut(k) {
                if s.size == size {
                    s.last_wanted = t;
                    continue;
                }
                s.kill();
                if let Some(sk) = &s.sound_key {
                    self.audio.stop(sk, 0.2);
                }
                sources.remove(k);
            }
            if let Some(src) = self.start(k, size) {
                sources.insert(k.clone(), src);
            }
        }
        let stale: Vec<String> = sources.iter().filter(|(k, s)| !wanted.contains_key(*k) && t - s.last_wanted > 1.5).map(|(k, _)| k.clone()).collect();
        for k in stale {
            if let Some(s) = sources.remove(&k) {
                s.kill();
                if let Some(sk) = &s.sound_key {
                    self.audio.stop(sk, 0.4);
                }
            }
        }
    }

    pub fn waiting_for(&self, key: &str) -> Option<f64> {
        let s = self.sources.lock().unwrap();
        let v = s.get(&Self::base_key(key))?;
        if v.latest.lock().unwrap().is_some() { None } else { Some(now() - v.started) }
    }

    pub fn restart(&self, key: &str) {
        let base = Self::base_key(key);
        if let Some(s) = self.sources.lock().unwrap().remove(&base) {
            s.kill();
        }
        self.errors.lock().unwrap().remove(key);
        self.errors.lock().unwrap().remove(&base);
    }

    pub fn stop_all(&self) {
        self.closing.store(true, Ordering::SeqCst);
        let mut sources = self.sources.lock().unwrap();
        for (_, s) in sources.drain() {
            s.kill();
        }
    }

    fn start(&self, key: &str, size: (u32, u32)) -> Option<VideoSource> {
        if self.closing.load(Ordering::SeqCst) {
            return None;
        }
        let ff = self.ffmpeg.read().unwrap().clone()?;
        let latest: Arc<Mutex<Option<Arc<VideoFrame>>>> = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        if let Some(spec) = key.strip_prefix("c:") {
            let screen = spec.starts_with("screen:") || spec.starts_with("window:");
            if screen && crate::permissions::screen_allowed() == Some(false) {
                self.errors.lock().unwrap().insert(key.to_string(), PERMISSION_MSG.into());
                return Some(VideoSource { latest, stop, child, last_wanted: now(), started: now(), size, sound_key: None });
            }
            refresh_mac_devices(&ff);
        }
        let (w, h) = size;
        let mut sound_key = None;
        let (is_camera, input_variants): (bool, Vec<(Vec<String>, Option<String>)>) = if let Some(spec) = key.strip_prefix("c:") {
            (true, (0..3).map(|i| crate::capture::input_args(spec, i)).collect())
        } else if let Some(rest) = key.strip_prefix("v:") {
            let looping = rest.as_bytes().first() == Some(&b'1');
            let sound = rest.as_bytes().get(1) == Some(&b'1');
            let path = rest.get(3..).unwrap_or("").to_string();
            let mut a: Vec<String> = vec!["-re".into()];
            if looping {
                a.extend(["-stream_loop", "-1"].map(String::from));
            }
            a.push("-i".into());
            a.push(path.clone());
            if sound {
                let sk = format!("media:{key}");
                self.audio.play(&sk, &path, SrcKind::Media, looping, 1.0, 0.15);
                sound_key = Some(sk);
            }
            (false, vec![(a, None)])
        } else {
            return None;
        };
        let vf_base = format!("scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,format=rgba");
        let latest2 = latest.clone();
        let stop2 = stop.clone();
        let child2 = child.clone();
        let errors_key = key.to_string();
        let errs2 = self.errors.clone();
        errs2.lock().unwrap().remove(key);
        std::thread::spawn(move || {
            for (attempt, (input, pre)) in input_variants.iter().enumerate() {
                let vf = match pre {
                    Some(p) => format!("{p},{vf_base}"),
                    None => vf_base.clone(),
                };
                if stop2.load(Ordering::SeqCst) {
                    return;
                }
                let mut cmd = ffmpeg_command(&ff);
                cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
                if is_camera {
                    cmd.args(["-fflags", "nobuffer"]);
                }
                cmd.args(input);
                cmd.args(["-an", "-vf", &vf, "-r", "30", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"]);
                cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
                let Ok(mut c) = cmd.spawn() else {
                    errs2.lock().unwrap().insert(errors_key.clone(), "Could not start FFmpeg".into());
                    return;
                };
                let mut out = c.stdout.take().unwrap();
                let err = c.stderr.take().unwrap();
                let errs3 = errs2.clone();
                let ek = errors_key.clone();
                std::thread::spawn(move || {
                    for line in BufReader::new(err).lines().map_while(Result::ok) {
                        if let Some(line) = useful_error(&line) {
                            let mut e = errs3.lock().unwrap();
                            if !(is_generic_error(&line) && e.contains_key(&ek)) {
                                e.insert(ek.clone(), line);
                            }
                        }
                    }
                });
                *child2.lock().unwrap() = Some(c);
                let n = (w * h * 4) as usize;
                let mut got = 0u64;
                loop {
                    if stop2.load(Ordering::SeqCst) {
                        if let Some(mut c) = child2.lock().unwrap().take() {
                            let _ = c.kill();
                            let _ = c.wait();
                        }
                        return;
                    }
                    let mut buf = vec![0u8; n];
                    if out.read_exact(&mut buf).is_err() {
                        break;
                    }
                    got += 1;
                    if got == 1 {
                        errs2.lock().unwrap().remove(&errors_key);
                    }
                    *latest2.lock().unwrap() = Some(Arc::new(VideoFrame { w, h, data: buf }));
                }
                if let Some(mut c) = child2.lock().unwrap().take() {
                    let _ = c.wait();
                }
                if got > 0 || attempt + 1 == input_variants.len() {
                    return;
                }
            }
        });
        Some(VideoSource { latest, stop, child, last_wanted: now(), started: now(), size, sound_key })
    }

}

type ImgResult = Result<Arc<image::RgbaImage>, ()>;

pub struct ImageStore {
    map: Arc<Mutex<HashMap<String, Option<ImgResult>>>>,
}

impl ImageStore {
    pub fn new() -> Self {
        ImageStore { map: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub fn get(&self, path: &str) -> Option<ImgResult> {
        let mut m = self.map.lock().unwrap();
        match m.get(path) {
            Some(Some(r)) => return Some(r.clone()),
            Some(None) => return None,
            None => {}
        }
        m.insert(path.to_string(), None);
        let map = self.map.clone();
        let p = path.to_string();
        std::thread::spawn(move || {
            let r = image::open(&p)
                .map(|img| {
                    let img = if img.width() > 2560 || img.height() > 1600 { img.resize(2560, 1600, image::imageops::FilterType::Triangle) } else { img };
                    Arc::new(img.to_rgba8())
                })
                .map_err(|_| ());
            map.lock().unwrap().insert(p, Some(r));
        });
        None
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    #[test]
    fn hides_noise_lines() {
        assert_eq!(useful_error("[in#0 @ 0x7709010000]   bgr0"), None);
        assert_eq!(useful_error("objc[123]: class `NSKVONotifying_AVCaptureScreenInput' not linked into application"), None);
        assert!(useful_error("[avfoundation @ 0x1] Selected pixel format (yuv420p) is not supported by the input device.").unwrap().contains("trying another"));
        assert!(is_generic_error(&useful_error("Error opening input files: Input/output error").unwrap()));
        assert!(useful_error("[AVFoundation indev @ 0x1] Invalid device index").unwrap().contains("isn't connected"));
    }

    #[test]
    #[ignore]
    fn captures_mac_screens() {
        let ff = locate_ffmpeg("").expect("ffmpeg installed").path;
        let cams = list_cameras(&ff);
        println!("{cams:?}");
        for c in cams.iter().filter(|c| c.screen) {
            let mut ok = false;
            for attempt in 0..3 {
                let (args, _) = crate::capture::input_args(&c.spec, attempt);
                let out = ffmpeg_command(&ff).args(["-hide_banner", "-loglevel", "error", "-nostdin"]).args(&args).args(["-frames:v", "2", "-f", "null", "-"]).output().unwrap();
                println!("{} attempt {attempt}: {}", c.name, out.status);
                if out.status.success() {
                    ok = true;
                    break;
                }
            }
            assert!(ok, "{} gave no picture", c.name);
        }
    }
}
