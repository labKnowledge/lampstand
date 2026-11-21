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
    pub stats_period: bool,
}

impl FfmpegInfo {
    pub fn has_stats_period(&self) -> bool {
        self.stats_period
    }
}

fn accepts_stats_period(path: &Path) -> bool {
    ffmpeg_command(path).args(["-hide_banner", "-stats_period", "1", "-version"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

fn capture_devices() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["avfoundation"]
    } else if cfg!(windows) {
        &["dshow", "gdigrab"]
    } else {
        &["v4l2", "x11grab"]
    }
}

fn input_devices(path: &Path) -> Vec<String> {
    let Ok(out) = ffmpeg_command(path).args(["-hide_banner", "-devices"]).stdin(Stdio::null()).output() else { return vec![] };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            let flags = words.next()?;
            let names = words.next()?;
            flags.starts_with('D').then(|| names.split(',').map(String::from).collect::<Vec<_>>())
        })
        .flatten()
        .collect()
}

fn probe_ffmpeg(p: PathBuf) -> Option<(FfmpegInfo, usize)> {
    if !p.is_file() {
        return None;
    }
    let out = ffmpeg_command(&p).arg("-hide_banner").arg("-version").stdin(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
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
    let stats_period = accepts_stats_period(&p);
    let devices = input_devices(&p);
    let score = capture_devices().iter().filter(|d| devices.iter().any(|x| x == *d)).count() * 4 + encoders.iter().any(|e| e == "libx264") as usize * 2 + stats_period as usize;
    Some((FfmpegInfo { path: p, version, encoders, stats_period }, score))
}

pub fn locate_ffmpeg(custom: &str) -> Option<FfmpegInfo> {
    if !custom.trim().is_empty() {
        if let Some((info, _)) = probe_ffmpeg(PathBuf::from(custom.trim())) {
            return Some(info);
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut best: Option<(FfmpegInfo, usize)> = None;
    for p in candidates() {
        let real = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        if !seen.insert(real) {
            continue;
        }
        if let Some((info, score)) = probe_ffmpeg(p) {
            if best.as_ref().map(|(_, s)| score > *s).unwrap_or(true) {
                best = Some((info, score));
            }
        }
    }
    best.map(|(info, _)| info)
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

pub fn parse_avfoundation(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut in_video = false;
    for line in text.lines() {
        if line.contains("video devices") {
            in_video = true;
            continue;
        }
        if line.contains("audio devices") {
            break;
        }
        if !in_video {
            continue;
        }
        let Some(rest) = line.split("] [").nth(1) else { continue };
        if let Some((idx, name)) = rest.split_once("] ") {
            let name = name.trim();
            if !name.is_empty() {
                out.push((idx.trim().to_string(), name.to_string()));
            }
        }
    }
    out
}

fn quoted(line: &str) -> Option<&str> {
    let start = line.find('"')? + 1;
    let len = line[start..].find('"')?;
    Some(&line[start..start + len])
}

pub fn parse_dshow(text: &str) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    let mut section_video = false;
    let mut last_was_video = false;
    for line in text.lines() {
        if line.contains("DirectShow video devices") {
            section_video = true;
            continue;
        }
        if line.contains("DirectShow audio devices") {
            section_video = false;
            continue;
        }
        let Some(name) = quoted(line) else { continue };
        if line.contains("Alternative name") {
            if last_was_video {
                if let Some(last) = out.last_mut() {
                    last.1 = Some(name.to_string());
                }
            }
            continue;
        }
        let tail = &line[line.rfind('"').unwrap_or(0)..];
        let video = match (tail.find('('), tail.rfind(')')) {
            (Some(a), Some(b)) if a < b => tail[a..b].contains("video"),
            _ => section_video,
        };
        last_was_video = video;
        if video {
            out.push((name.to_string(), None));
        }
    }
    out
}

fn number_duplicates(list: &mut [(String, String)]) {
    let names: Vec<String> = list.iter().map(|(_, n)| n.clone()).collect();
    for (i, (_, name)) in list.iter_mut().enumerate() {
        let total = names.iter().filter(|n| **n == names[i]).count();
        if total > 1 {
            let nth = names[..i].iter().filter(|n| **n == names[i]).count() + 1;
            *name = format!("{} ({nth})", names[i]);
        }
    }
}

fn mac_cameras(ffmpeg: &Path) -> Vec<CameraDevice> {
    let Ok(o) = ffmpeg_command(ffmpeg).args(["-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""]).stdin(Stdio::null()).output() else { return vec![] };
    let found = parse_avfoundation(&String::from_utf8_lossy(&o.stderr));
    let mut names: HashMap<String, String> = HashMap::new();
    for (idx, name) in &found {
        if found.iter().filter(|(_, n)| n == name).count() == 1 {
            names.insert(name.clone(), idx.clone());
        }
    }
    *MAC_DEVICES.lock().unwrap() = Some((std::time::Instant::now(), names));
    let mut screens = Vec::new();
    let mut cams: Vec<(String, String)> = Vec::new();
    for (idx, name) in found {
        match name.strip_prefix("Capture screen ").and_then(|n| n.trim().parse::<usize>().ok()) {
            Some(ord) => {
                let shown = format!("Screen {}", ord + 1);
                screens.push(CameraDevice { spec: format!("screen:@{ord}|{shown}"), name: shown, screen: true });
            }
            None => cams.push((idx, name)),
        }
    }
    number_duplicates(&mut cams);
    let mut out: Vec<CameraDevice> = cams.into_iter().map(|(id, name)| CameraDevice { spec: format!("{id}|{name}"), name, screen: false }).collect();
    out.extend(screens);
    out
}

fn windows_cameras(ffmpeg: &Path) -> Vec<CameraDevice> {
    let Ok(o) = ffmpeg_command(ffmpeg).args(["-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"]).stdin(Stdio::null()).output() else { return vec![] };
    let found = parse_dshow(&String::from_utf8_lossy(&o.stderr));
    let mut cams: Vec<(String, String)> = found
        .iter()
        .map(|(name, alt)| {
            let shared = found.iter().filter(|(n, _)| n == name).count() > 1;
            let id = match alt {
                Some(a) if shared => a.clone(),
                _ => name.clone(),
            };
            (id, name.clone())
        })
        .collect();
    number_duplicates(&mut cams);
    let mut out: Vec<CameraDevice> = cams.into_iter().map(|(id, name)| CameraDevice { spec: format!("{id}|{name}"), name, screen: false }).collect();
    out.push(CameraDevice { spec: "screen:desktop|Whole desktop".into(), name: "Whole desktop".into(), screen: true });
    out
}

enum V4l2Node {
    Capture(String),
    NotCapture,
    Unknown,
}

#[cfg(target_os = "linux")]
fn probe_v4l2(path: &str) -> V4l2Node {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    #[repr(C)]
    struct Capability {
        driver: [u8; 16],
        card: [u8; 32],
        bus_info: [u8; 32],
        version: u32,
        capabilities: u32,
        device_caps: u32,
        reserved: [u32; 3],
    }
    const VIDIOC_QUERYCAP: u64 = 0x8068_5600;
    const DEVICE_CAPS: u32 = 0x8000_0000;
    const VIDEO_CAPTURE: u32 = 0x0000_0001;
    const VIDEO_CAPTURE_MPLANE: u32 = 0x0000_1000;
    let Ok(file) = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(path) else { return V4l2Node::Unknown };
    let mut cap = Capability { driver: [0; 16], card: [0; 32], bus_info: [0; 32], version: 0, capabilities: 0, device_caps: 0, reserved: [0; 3] };
    if unsafe { libc::ioctl(file.as_raw_fd(), VIDIOC_QUERYCAP as _, &mut cap) } != 0 {
        return V4l2Node::Unknown;
    }
    let caps = if cap.capabilities & DEVICE_CAPS != 0 { cap.device_caps } else { cap.capabilities };
    if caps & (VIDEO_CAPTURE | VIDEO_CAPTURE_MPLANE) == 0 {
        return V4l2Node::NotCapture;
    }
    let end = cap.card.iter().position(|b| *b == 0).unwrap_or(cap.card.len());
    V4l2Node::Capture(String::from_utf8_lossy(&cap.card[..end]).trim().to_string())
}

#[cfg(not(target_os = "linux"))]
fn probe_v4l2(_path: &str) -> V4l2Node {
    V4l2Node::Unknown
}

fn video_node_number(name: &str) -> Option<u32> {
    name.strip_prefix("video")?.parse().ok()
}

fn stable_v4l2_paths() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for dir in ["/dev/v4l/by-path", "/dev/v4l/by-id"] {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            if let Ok(target) = std::fs::canonicalize(e.path()) {
                map.insert(target.to_string_lossy().to_string(), e.path().to_string_lossy().to_string());
            }
        }
    }
    map
}

fn linux_cameras() -> Vec<CameraDevice> {
    let stable = stable_v4l2_paths();
    let sys = Path::new("/sys/class/video4linux");
    let dir = if sys.is_dir() { sys } else { Path::new("/dev") };
    let mut nodes: Vec<(u32, String)> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().filter_map(|e| { let n = e.file_name().to_string_lossy().to_string(); video_node_number(&n).map(|k| (k, n)) }).collect())
        .unwrap_or_default();
    nodes.sort();
    let mut cams: Vec<(String, String)> = Vec::new();
    for (_, node) in nodes {
        let path = format!("/dev/{node}");
        let sys_name = std::fs::read_to_string(sys.join(&node).join("name")).map(|s| s.trim().to_string()).ok().filter(|s| !s.is_empty());
        let name = match probe_v4l2(&path) {
            V4l2Node::Capture(card) => sys_name.or(Some(card).filter(|c| !c.is_empty())),
            V4l2Node::NotCapture => continue,
            V4l2Node::Unknown => {
                let index = std::fs::read_to_string(sys.join(&node).join("index")).ok().and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0);
                if index != 0 {
                    continue;
                }
                sys_name
            }
        };
        let id = stable.get(&path).cloned().unwrap_or(path);
        cams.push((id, name.unwrap_or_else(|| node.clone())));
    }
    number_duplicates(&mut cams);
    let mut out: Vec<CameraDevice> = cams.into_iter().map(|(id, name)| CameraDevice { spec: format!("{id}|{name}"), name, screen: false }).collect();
    let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
    out.push(CameraDevice { spec: format!("screen:{display}|Screen {display}"), name: format!("Screen {display}"), screen: true });
    out
}

pub const CAMERA_ACCESS_RULE: &str = include_str!("../linux/70-lampstand-cameras.rules");
pub const CAMERA_ACCESS_RULE_PATH: &str = "/etc/udev/rules.d/70-lampstand-cameras.rules";

#[derive(Clone, Debug, PartialEq)]
pub struct StuckCamera {
    pub name: String,
    pub port: String,
    pub bus: u32,
    pub dev: u32,
}

impl StuckCamera {
    pub fn key(&self) -> String {
        format!("{}:{}", self.port, self.dev)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Restart {
    Done,
    NeedsPermission,
    Failed,
}

fn is_usb_port_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '.') && name.contains('-')
}

pub fn cameras_without_driver() -> Vec<StuckCamera> {
    if cfg!(target_os = "macos") || cfg!(windows) {
        return vec![];
    }
    let Ok(rd) = std::fs::read_dir("/sys/bus/usb/devices") else { return vec![] };
    let mut stuck = Vec::new();
    for dev in rd.flatten().map(|e| e.path()) {
        let port = dev.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if !is_usb_port_name(&port) {
            continue;
        }
        let Ok(ifaces) = std::fs::read_dir(&dev) else { continue };
        let video_without_driver = ifaces.flatten().map(|e| e.path()).any(|i| {
            let read = |f: &str| std::fs::read_to_string(i.join(f)).map(|s| s.trim().to_string()).unwrap_or_default();
            read("bInterfaceClass") == "0e" && read("bInterfaceSubClass") == "01" && !i.join("driver").exists()
        });
        if !video_without_driver {
            continue;
        }
        let read = |f: &str| std::fs::read_to_string(dev.join(f)).map(|s| s.trim().to_string()).unwrap_or_default();
        let (Ok(bus), Ok(devnum)) = (read("busnum").parse(), read("devnum").parse()) else { continue };
        let name = read("product");
        stuck.push(StuckCamera { name: if name.is_empty() { "USB camera".into() } else { name }, port, bus, dev: devnum });
    }
    stuck
}

#[cfg(target_os = "linux")]
pub fn restart_usb_camera(cam: &StuckCamera) -> Restart {
    use std::os::unix::io::AsRawFd;
    #[repr(C)]
    struct UsbIoctl {
        ifno: libc::c_int,
        ioctl_code: libc::c_int,
        data: *mut libc::c_void,
    }
    const USBDEVFS_RESET: u64 = 0x5514;
    const USBDEVFS_CONNECT: libc::c_int = 0x5517;
    const USBDEVFS_IOCTL: u64 = 0xC000_5512 | ((std::mem::size_of::<UsbIoctl>() as u64) << 16);
    let path = format!("/dev/bus/usb/{:03}/{:03}", cam.bus, cam.dev);
    let file = match std::fs::OpenOptions::new().read(true).write(true).open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Restart::NeedsPermission,
        Err(_) => return Restart::Failed,
    };
    let fd = file.as_raw_fd();
    unsafe { libc::ioctl(fd, USBDEVFS_RESET as _, 0) };
    std::thread::sleep(std::time::Duration::from_millis(600));
    let mut connected = false;
    for ifno in 0..2 {
        let mut req = UsbIoctl { ifno, ioctl_code: USBDEVFS_CONNECT, data: std::ptr::null_mut() };
        connected |= unsafe { libc::ioctl(fd, USBDEVFS_IOCTL as _, &mut req) } == 0;
    }
    if connected { Restart::Done } else { Restart::Failed }
}

#[cfg(not(target_os = "linux"))]
pub fn restart_usb_camera(_cam: &StuckCamera) -> Restart {
    Restart::Failed
}

pub fn grant_camera_access_and_restart(cams: &[StuckCamera]) -> Result<(), String> {
    let rule = std::env::temp_dir().join("70-lampstand-cameras.rules");
    std::fs::write(&rule, CAMERA_ACCESS_RULE).map_err(|e| e.to_string())?;
    let mut script = format!(
        "install -m644 '{}' {CAMERA_ACCESS_RULE_PATH} && udevadm control --reload-rules && udevadm trigger --subsystem-match=usb --action=change",
        rule.display().to_string().replace('\'', "")
    );
    for c in cams.iter().filter(|c| is_usb_port_name(&c.port)) {
        script.push_str(&format!(" ; echo 0 > /sys/bus/usb/devices/{0}/authorized ; sleep 1 ; echo 1 > /sys/bus/usb/devices/{0}/authorized", c.port));
    }
    let status = Command::new("pkexec").args(["sh", "-c", &script]).stdin(Stdio::null()).status().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&rule);
    match status.code() {
        Some(0) => Ok(()),
        Some(126) | Some(127) => Err("cancelled".into()),
        _ => Err("the system didn't allow it".into()),
    }
}

pub fn list_cameras(ffmpeg: &Path) -> Vec<CameraDevice> {
    if cfg!(target_os = "macos") {
        mac_cameras(ffmpeg)
    } else if cfg!(windows) {
        windows_cameras(ffmpeg)
    } else {
        linux_cameras()
    }
}

pub const PERMISSION_MSG: &str = "macOS blocked screen recording for Lampstand: open Cameras & screens and follow the three steps";

const GENERIC_MSG: &str = "Couldn't open this screen or camera";
const GENERIC_VIDEO_MSG: &str = "Couldn't open this video file";
const GONE_MSG: &str = "That camera isn't connected any more. Plug it back in; Lampstand finds it again by itself";
const BUSY_MSG: &str = "Another app is using this camera (Zoom, Teams, OBS or a browser tab). Close it, then press Try again";

pub fn is_generic_error(line: &str) -> bool {
    line == GENERIC_MSG || line == GENERIC_VIDEO_MSG
}

fn permission_msg(screen: bool) -> &'static str {
    match (cfg!(target_os = "macos"), cfg!(windows), screen) {
        (true, _, true) => PERMISSION_MSG,
        (true, _, false) => "macOS blocked the camera for Lampstand: open System Settings, Privacy & Security, Camera, and switch Lampstand on",
        (_, true, _) => "Windows blocked the camera: open Settings, Privacy & security, Camera, and allow desktop apps to use it",
        (_, _, true) => "This desktop blocked screen capture. Log in with an X11 session; Wayland sessions block it",
        _ => "Lampstand isn't allowed to open the camera. Run  sudo usermod -aG video $USER  then log out and back in",
    }
}

pub fn useful_error(line: &str, key: &str) -> Option<String> {
    let l = line.trim();
    if l.is_empty() || l.contains("NSKVONotifying") || l.starts_with("objc[") || l.contains("falling back to default") {
        return None;
    }
    let video = key.starts_with("v:");
    let screen = is_screen_key(key);
    let lower = l.to_lowercase();
    if !video {
        if l.contains("Invalid device index") || l.contains("Video device not found") || l.contains("Could not find video device") || (!screen && l.contains("No such file or directory")) {
            return Some(if screen { "That screen isn't connected any more. Choose it again".into() } else { GONE_MSG.into() });
        }
        if lower.contains("device or resource busy") || lower.contains("could not run graph") || lower.contains("could not run filter") || lower.contains("already in use") {
            return Some(BUSY_MSG.into());
        }
        if lower.contains("option not found") || lower.contains("unknown input format") || lower.contains("unrecognized option") {
            return Some("This copy of FFmpeg can't capture screens or cameras. Install the full FFmpeg, or choose it in Stream setup".into());
        }
        if lower.contains("not authorized") || lower.contains("denied") || lower.contains("failed to create av capture input") || lower.contains("permission") {
            return Some(permission_msg(screen).into());
        }
    }
    if l.contains("Error opening input") {
        return Some(if video { GENERIC_VIDEO_MSG } else { GENERIC_MSG }.into());
    }
    let body = l.rsplit("] ").next().unwrap_or(l).trim();
    if !body.contains(' ') && body.len() <= 12 {
        return None;
    }
    if body.starts_with("Supported") || body.contains("Supported pixel formats") || body.contains("Supported modes") || body.contains("Cannot find a proper format") {
        return None;
    }
    if body.contains("pixel format") && body.contains("not supported") {
        return Some("The device doesn't offer the requested picture format; trying another one".into());
    }
    if body.contains("framerate") && body.contains("not supported") {
        return Some("The device doesn't offer this frame rate; trying another one".into());
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
        }
        let (w, h) = size;
        let mut sound_key = None;
        let camera_spec = key.strip_prefix("c:").map(String::from);
        let is_camera = camera_spec.is_some();
        let file_input: Vec<(Vec<String>, Option<String>)> = if is_camera {
            vec![]
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
            vec![(a, None)]
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
            let input_variants = match &camera_spec {
                Some(spec) => {
                    refresh_mac_devices(&ff);
                    crate::capture::input_variants(spec)
                }
                None => file_input,
            };
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
                        if let Some(line) = useful_error(&line, &ek) {
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

const MAX_IMAGES: usize = 32;

pub struct ImageStore {
    map: Arc<Mutex<HashMap<String, (Option<ImgResult>, f64)>>>,
}

impl ImageStore {
    pub fn new() -> Self {
        ImageStore { map: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub fn get(&self, path: &str) -> Option<ImgResult> {
        let mut m = self.map.lock().unwrap();
        let t = now();
        if let Some((r, used)) = m.get_mut(path) {
            *used = t;
            return r.clone();
        }
        if m.len() >= MAX_IMAGES {
            let mut loaded: Vec<(String, f64)> = m.iter().filter(|(_, (r, u))| r.is_some() && t - *u > 5.0).map(|(k, (_, u))| (k.clone(), *u)).collect();
            loaded.sort_by(|a, b| a.1.total_cmp(&b.1));
            for (k, _) in loaded.into_iter().take(MAX_IMAGES / 2) {
                m.remove(&k);
            }
        }
        m.insert(path.to_string(), (None, t));
        let map = self.map.clone();
        let p = path.to_string();
        std::thread::spawn(move || {
            let r = image::open(&p)
                .map(|img| {
                    let img = if img.width() > 2560 || img.height() > 1600 { img.resize(2560, 1600, image::imageops::FilterType::Triangle) } else { img };
                    Arc::new(img.to_rgba8())
                })
                .map_err(|_| ());
            if let Some(entry) = map.lock().unwrap().get_mut(&p) {
                entry.0 = Some(r);
            }
        });
        None
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    #[test]
    fn hides_noise_lines() {
        let cam = "c:/dev/video0|Cam";
        assert_eq!(useful_error("[in#0 @ 0x7709010000]   bgr0", cam), None);
        assert_eq!(useful_error("objc[123]: class `NSKVONotifying_AVCaptureScreenInput' not linked into application", "c:screen:@0|Screen 1"), None);
        assert!(useful_error("[avfoundation @ 0x1] Selected pixel format (yuv420p) is not supported by the input device.", cam).unwrap().contains("trying another"));
        assert!(is_generic_error(&useful_error("Error opening input files: Input/output error", cam).unwrap()));
        assert!(useful_error("[AVFoundation indev @ 0x1] Invalid device index", cam).unwrap().contains("isn't connected"));
    }

    #[test]
    fn explains_busy_missing_and_blocked_cameras() {
        let cam = "c:/dev/video0|Cam";
        assert_eq!(useful_error("[video4linux2,v4l2 @ 0x1] ioctl(VIDIOC_STREAMON): Device or resource busy", cam).as_deref(), Some(BUSY_MSG));
        assert_eq!(useful_error("[dshow @ 0x1] Could not run graph (sometimes caused by a device already in use by other application)", "c:USB Cam|USB Cam").as_deref(), Some(BUSY_MSG));
        assert_eq!(useful_error("[dshow @ 0x1] Could not find video device with name [HD Cam] among source devices of type video.", "c:HD Cam|HD Cam").as_deref(), Some(GONE_MSG));
        assert_eq!(useful_error("/dev/video4: No such file or directory", cam).as_deref(), Some(GONE_MSG));
        assert!(!useful_error("/dev/video4: Permission denied", cam).unwrap().contains("screen recording"));
        assert_eq!(useful_error("clip.mp4: No such file or directory", "v:11:clip.mp4").as_deref(), Some("clip.mp4: No such file or directory"));
        assert_eq!(useful_error("Error opening input file clip.mp4.", "v:11:clip.mp4").as_deref(), Some(GENERIC_VIDEO_MSG));
        assert!(useful_error("Error splitting the argument list: Option not found", "c:screen::0|Screen :0").unwrap().contains("full FFmpeg"));
    }

    #[test]
    fn reads_dshow_lists_old_and_new() {
        let new = [
            r#"[dshow @ 000001] "Integrated Camera" (video)"#,
            r#"[dshow @ 000001]   Alternative name "@device_pnp_\\?\usb#vid_04f2&pid_b6d9&mi_00#6&1#{65e8773d}\global""#,
            r#"[dshow @ 000001] "USB Video" (video)"#,
            r#"[dshow @ 000001]   Alternative name "@device_pnp_\\?\usb#vid_534d#a""#,
            r#"[dshow @ 000001] "USB Video" (video)"#,
            r#"[dshow @ 000001]   Alternative name "@device_pnp_\\?\usb#vid_534d#b""#,
            r#"[dshow @ 000001] "OBS Virtual Camera" (none)"#,
            r#"[dshow @ 000001] "Microphone (Realtek(R) Audio)" (audio)"#,
            r#"[dshow @ 000001]   Alternative name "@device_cm_{33D9A762}\wave_{A}""#,
            "dummy: Immediate exit requested",
        ]
        .join("\n");
        let got = parse_dshow(&new);
        assert_eq!(got.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["Integrated Camera", "USB Video", "USB Video"]);
        assert!(got[2].1.as_deref().unwrap().ends_with("#b"));
        let old = [
            "[dshow @ 0x1] DirectShow video devices (some may be both video and audio devices)",
            r#"[dshow @ 0x1]  "HD Pro Webcam C920""#,
            r#"[dshow @ 0x1]     Alternative name "@device_pnp_\\?\usb#vid_046d&pid_082d""#,
            "[dshow @ 0x1] DirectShow audio devices",
            r#"[dshow @ 0x1]  "Microphone (HD Pro Webcam C920)""#,
            r#"[dshow @ 0x1]     Alternative name "@device_cm_{33D9A762}\wave_{B}""#,
        ]
        .join("\n");
        let got = parse_dshow(&old);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "HD Pro Webcam C920");
        assert!(got[0].1.as_deref().unwrap().contains("082d"));
    }

    #[test]
    fn reads_avfoundation_list() {
        let text = [
            "[AVFoundation indev @ 0x7f] AVFoundation video devices:",
            "[AVFoundation indev @ 0x7f] [0] FaceTime HD Camera",
            "[AVFoundation indev @ 0x7f] [1] Cam Link 4K",
            "[AVFoundation indev @ 0x7f] [2] Capture screen 0",
            "[AVFoundation indev @ 0x7f] AVFoundation audio devices:",
            "[AVFoundation indev @ 0x7f] [0] MacBook Pro Microphone",
        ]
        .join("\n");
        let got = parse_avfoundation(&text);
        let want: Vec<(String, String)> = [("0", "FaceTime HD Camera"), ("1", "Cam Link 4K"), ("2", "Capture screen 0")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn numbers_cameras_with_the_same_name() {
        let mut list = vec![("a".to_string(), "USB Video".to_string()), ("b".into(), "Brio".into()), ("c".into(), "USB Video".into())];
        number_duplicates(&mut list);
        assert_eq!(list.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(), ["USB Video (1)", "Brio", "USB Video (2)"]);
    }

    #[test]
    fn sorts_video_nodes_by_number() {
        let mut nodes: Vec<(u32, &str)> = ["video10", "video2", "video0", "vbi0"].iter().filter_map(|n| video_node_number(n).map(|k| (k, *n))).collect();
        nodes.sort();
        assert_eq!(nodes.iter().map(|(_, n)| *n).collect::<Vec<_>>(), ["video0", "video2", "video10"]);
    }

    #[test]
    #[ignore]
    fn picks_the_most_capable_ffmpeg() {
        let info = locate_ffmpeg("").expect("an ffmpeg");
        println!("picked {} ({}) stats_period={}", info.path.display(), info.version, info.stats_period);
    }

    #[test]
    #[ignore]
    fn detects_stats_period_support() {
        for p in ["/usr/bin/ffmpeg", "ffmpeg"] {
            println!("{p}: {}", accepts_stats_period(Path::new(p)));
        }
    }

    #[test]
    fn lists_this_machines_cameras() {
        let found = linux_cameras();
        println!("{found:?}");
        assert!(found.iter().any(|c| c.screen));
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
