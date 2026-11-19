use crate::media::{list_cameras, CameraDevice};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

pub static SHOW_CURSOR: AtomicBool = AtomicBool::new(true);

fn cursor_flag() -> &'static str {
    if SHOW_CURSOR.load(Ordering::Relaxed) { "1" } else { "0" }
}

fn parse_rect(s: &str) -> Option<(i64, i64, i64, i64)> {
    let v: Vec<i64> = s.split(',').filter_map(|x| x.trim().parse::<f64>().ok().map(|f| f.round() as i64)).collect();
    if v.len() == 4 { Some((v[0], v[1], v[2], v[3])) } else { None }
}

fn even(v: i64) -> i64 {
    (v / 2) * 2
}

fn mac_screen_format(attempt: usize) -> Vec<String> {
    match attempt {
        0 => ["-framerate", "30", "-pixel_format", "bgr0"].map(String::from).to_vec(),
        1 => ["-pixel_format", "bgr0"].map(String::from).to_vec(),
        _ => ["-framerate", "30", "-pixel_format", "nv12"].map(String::from).to_vec(),
    }
}

fn mac_screen_index(screen: &str, label: &str) -> String {
    if let Some(n) = screen.strip_prefix('@') {
        return crate::media::mac_device_index(&format!("Capture screen {n}")).unwrap_or_else(|| n.to_string());
    }
    if let Some(k) = label.strip_prefix("Screen ").and_then(|k| k.trim().parse::<usize>().ok()) {
        if let Some(i) = crate::media::mac_device_index(&format!("Capture screen {}", k.saturating_sub(1))) {
            return i;
        }
    }
    screen.to_string()
}

pub fn input_args(spec: &str, attempt: usize) -> (Vec<String>, Option<String>) {
    let id = spec.split_once('|').map(|(a, _)| a).unwrap_or(spec).to_string();
    let label = spec.split_once('|').map(|(_, b)| b).unwrap_or("").to_string();
    let mut a: Vec<String> = Vec::new();
    if let Some(screen) = id.strip_prefix("screen:") {
        if cfg!(target_os = "macos") {
            a.extend(["-f", "avfoundation", "-capture_cursor", cursor_flag()].map(String::from));
            a.extend(mac_screen_format(attempt));
            a.push("-i".into());
            a.push(format!("{}:none", mac_screen_index(screen, &label)));
        } else if cfg!(windows) {
            a.extend(["-f", "gdigrab", "-framerate", "30", "-draw_mouse", cursor_flag()].map(String::from));
            if let Some((x, y, w, h)) = screen.strip_prefix("mon:").and_then(parse_rect) {
                a.extend(["-offset_x".into(), x.to_string(), "-offset_y".into(), y.to_string(), "-video_size".into(), format!("{w}x{h}")]);
            }
            a.extend(["-i", "desktop"].map(String::from));
        } else {
            a.extend(["-f", "x11grab", "-framerate", "30", "-draw_mouse", cursor_flag()].map(String::from));
            let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
            if let Some((x, y, w, h)) = screen.strip_prefix("mon:").and_then(parse_rect) {
                a.extend(["-video_size".into(), format!("{w}x{h}"), "-i".into(), format!("{display}+{x},{y}")]);
            } else {
                a.extend(["-i".into(), screen.to_string()]);
            }
        }
        return (a, None);
    }
    if let Some(win) = id.strip_prefix("window:") {
        if cfg!(target_os = "macos") {
            let mut parts = win.splitn(2, ',');
            let dev = mac_screen_index(parts.next().unwrap_or("0"), "");
            let crop = parts.next().and_then(parse_rect).map(|(x, y, w, h)| format!("crop={}:{}:{}:{}", even(w.max(16)), even(h.max(16)), x.max(0), y.max(0)));
            a.extend(["-f", "avfoundation", "-capture_cursor", cursor_flag()].map(String::from));
            a.extend(mac_screen_format(attempt));
            a.push("-i".into());
            a.push(format!("{dev}:none"));
            return (a, crop);
        } else if cfg!(windows) {
            a.extend(["-f", "gdigrab", "-framerate", "30", "-draw_mouse", cursor_flag(), "-i"].map(String::from));
            a.push(format!("title={win}"));
        } else {
            let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
            a.extend(["-f", "x11grab", "-framerate", "30", "-draw_mouse", cursor_flag(), "-window_id"].map(String::from));
            a.push(win.to_string());
            a.push("-i".into());
            a.push(display);
        }
        return (a, None);
    }
    if cfg!(target_os = "macos") {
        a.extend(["-f", "avfoundation"].map(String::from));
        match attempt {
            0 => a.extend(["-framerate", "30", "-video_size", "1280x720", "-pixel_format", "uyvy422"].map(String::from)),
            1 => a.extend(["-framerate", "30", "-pixel_format", "uyvy422"].map(String::from)),
            _ => a.extend(["-pixel_format", "nv12"].map(String::from)),
        }
        let cam = crate::media::mac_device_index(&label).unwrap_or(id);
        a.push("-i".into());
        a.push(format!("{cam}:none"));
    } else if cfg!(windows) {
        a.extend(["-f", "dshow", "-rtbufsize", "100M"].map(String::from));
        if attempt == 0 {
            a.extend(["-video_size", "1280x720", "-framerate", "30"].map(String::from));
        }
        a.push("-i".into());
        a.push(format!("video={id}"));
    } else {
        a.extend(["-f", "v4l2"].map(String::from));
        if attempt == 0 {
            a.extend(["-framerate", "30", "-video_size", "1280x720"].map(String::from));
        }
        a.push("-i".into());
        a.push(id);
    }
    (a, None)
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureTarget {
    pub spec: String,
    pub name: String,
    pub detail: String,
    pub window: bool,
}

const MAC_WINDOWS_JS: &str = r#"ObjC.import('CoreGraphics');
ObjC.import('AppKit');
var opts = $.kCGWindowListOptionOnScreenOnly | $.kCGWindowListExcludeDesktopElements;
var list = ObjC.castRefToObject($.CGWindowListCopyWindowInfo(opts, $.kCGNullWindowID));
var out = [];
var scr = $.NSScreen.screens;
for (var s = 0; s < scr.count; s++) { var f = scr.objectAtIndex(s).frame; out.push("S\t" + s + "\t" + f.origin.x + "\t" + f.origin.y + "\t" + f.size.width + "\t" + f.size.height + "\t" + scr.objectAtIndex(s).backingScaleFactor); }
for (var i = 0; i < list.count; i++) {
  var w = list.objectAtIndex(i);
  if (w.objectForKey('kCGWindowLayer').js != 0) continue;
  var owner = w.objectForKey('kCGWindowOwnerName'); owner = owner ? owner.js : '';
  var name = w.objectForKey('kCGWindowName'); name = name ? name.js : '';
  var b = w.objectForKey('kCGWindowBounds');
  out.push("W\t" + owner + "\t" + name + "\t" + b.objectForKey('X').js + "\t" + b.objectForKey('Y').js + "\t" + b.objectForKey('Width').js + "\t" + b.objectForKey('Height').js);
}
out.join("\n");"#;

fn run_text(cmd: &str, args: &[&str]) -> String {
    #[allow(unused_mut)]
    let mut c = Command::new(cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c.args(args).stdin(Stdio::null()).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
}

fn mac_targets(ffmpeg: &Path) -> (Vec<CaptureTarget>, String) {
    let mut out = Vec::new();
    let screens: Vec<CameraDevice> = list_cameras(ffmpeg).into_iter().filter(|c| c.screen).collect();
    for s in &screens {
        out.push(CaptureTarget { spec: s.spec.clone(), name: s.name.clone(), detail: "Whole screen".into(), window: false });
    }
    let text = run_text("osascript", &["-l", "JavaScript", "-e", MAC_WINDOWS_JS]);
    let mut displays: Vec<(f64, f64, f64, f64, f64)> = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.first() == Some(&"S") && f.len() >= 7 {
            let n = |i: usize| f[i].parse::<f64>().unwrap_or(0.0);
            displays.push((n(2), n(3), n(4), n(5), n(6).max(1.0)));
        }
    }
    let main_h = displays.first().map(|d| d.3).unwrap_or(0.0);
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.first() != Some(&"W") || f.len() < 7 {
            continue;
        }
        let owner = f[1];
        let title = f[2];
        if owner.to_lowercase().contains("lampstand") {
            continue;
        }
        let n = |i: usize| f[i].parse::<f64>().unwrap_or(0.0);
        let (x, y, w, h) = (n(3), n(4), n(5), n(6));
        if w < 120.0 || h < 80.0 {
            continue;
        }
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let Some((di, d)) = displays.iter().enumerate().find(|(_, d)| {
            let top = main_h - (d.1 + d.3);
            cx >= d.0 && cx < d.0 + d.2 && cy >= top && cy < top + d.3
        }) else {
            continue;
        };
        let Some(dev) = screens.get(di) else { continue };
        let dev_id = dev.name.strip_prefix("Screen ").and_then(|k| k.trim().parse::<usize>().ok()).map(|k| format!("@{}", k.saturating_sub(1))).unwrap_or_else(|| dev.spec.trim_start_matches("screen:").split('|').next().unwrap_or("0").to_string());
        let top = main_h - (d.1 + d.3);
        let sc = d.4;
        let px = ((x - d.0) * sc).max(0.0);
        let py = ((y - top) * sc).max(0.0);
        let pw = (w * sc).min(d.2 * sc - px);
        let ph = (h * sc).min(d.3 * sc - py);
        let label = if title.is_empty() { owner.to_string() } else { format!("{owner} · {title}") };
        out.push(CaptureTarget {
            spec: format!("window:{dev_id},{px:.0},{py:.0},{pw:.0},{ph:.0},{:.0},{:.0}|{label}", d.2 * sc, d.3 * sc),
            name: label,
            detail: format!("Window on {} ({:.0}×{:.0})", dev.name, w, h),
            window: true,
        });
    }
    (out, "If a window moves after you pick it, pick it again. Window titles appear once Screen Recording access is allowed.".into())
}

fn windows_targets() -> (Vec<CaptureTarget>, String) {
    let mut out = vec![CaptureTarget { spec: "screen:desktop|All displays".into(), name: "All displays".into(), detail: "Every monitor together".into(), window: false }];
    let mons = run_text(
        "powershell",
        &["-NoProfile", "-Command", "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Screen]::AllScreens | ForEach-Object { \"$($_.Bounds.X),$($_.Bounds.Y),$($_.Bounds.Width),$($_.Bounds.Height),$($_.Primary)\" }"],
    );
    for (i, line) in mons.lines().enumerate() {
        let f: Vec<&str> = line.trim().split(',').collect();
        if f.len() >= 4 {
            let main = f.get(4).map(|p| p.trim() == "True").unwrap_or(false);
            let name = format!("Display {}{}", i + 1, if main { " (main)" } else { "" });
            out.push(CaptureTarget { spec: format!("screen:mon:{},{},{},{}|{}", f[0], f[1], f[2], f[3], name), name, detail: format!("{}×{}", f[2], f[3]), window: false });
        }
    }
    let wins = run_text("powershell", &["-NoProfile", "-Command", "Get-Process | Where-Object { $_.MainWindowTitle } | ForEach-Object { \"$($_.ProcessName)`t$($_.MainWindowTitle)\" }"]);
    for line in wins.lines() {
        if let Some((proc_name, title)) = line.trim().split_once('\t') {
            let title = title.trim();
            if proc_name.to_lowercase().contains("lampstand") || title.is_empty() {
                continue;
            }
            out.push(CaptureTarget { spec: format!("window:{title}|{title}"), name: title.to_string(), detail: format!("Window of {proc_name}"), window: true });
        }
    }
    (out, "Minimised windows show black. Keep the window open; it can sit behind others.".into())
}

fn linux_targets() -> (Vec<CaptureTarget>, String) {
    let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
    let mut out = vec![CaptureTarget { spec: format!("screen:{display}|All displays"), name: "All displays".into(), detail: display.clone(), window: false }];
    for line in run_text("xrandr", &["--listmonitors"]).lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 4 {
            continue;
        }
        let geo = f[2];
        let parsed = (|| -> Option<(String, String, String, String)> {
            let (wpart, rest) = geo.split_once('x')?;
            let w = wpart.split('/').next()?.to_string();
            let mut it = rest.split('+');
            let h = it.next()?.split('/').next()?.to_string();
            Some((w, h, it.next()?.to_string(), it.next()?.to_string()))
        })();
        if let Some((w, h, x, y)) = parsed {
            let name = f[3].to_string();
            out.push(CaptureTarget { spec: format!("screen:mon:{x},{y},{w},{h}|{name}"), name, detail: format!("{w}×{h}"), window: false });
        }
    }
    let wins = run_text("wmctrl", &["-l"]);
    for line in wins.lines() {
        let f: Vec<&str> = line.splitn(4, char::is_whitespace).filter(|x| !x.is_empty()).collect();
        if f.len() == 4 {
            let title = f[3].trim();
            out.push(CaptureTarget { spec: format!("window:{}|{}", f[0], title), name: title.to_string(), detail: "Window".into(), window: true });
        }
    }
    let note = if wins.is_empty() { "Install wmctrl to pick single windows. Wayland desktops may block screen capture; an X11 session works best." } else { "" };
    (out, note.into())
}

const MAC_DISPLAYS_JS: &str = r#"ObjC.import('AppKit');
var scr = $.NSScreen.screens;
var out = [];
for (var s = 0; s < scr.count; s++) {
  var d = scr.objectAtIndex(s);
  var f = d.frame;
  var n = d.localizedName ? d.localizedName.js : ('Display ' + (s + 1));
  out.push(n + "\t" + Math.round(f.size.width * d.backingScaleFactor) + "x" + Math.round(f.size.height * d.backingScaleFactor) + "\t" + f.origin.x + "\t" + f.origin.y + "\t" + f.size.width + "\t" + f.size.height);
}
out.join("\n");"#;

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayInfo {
    pub name: String,
    pub rect: Option<[f32; 4]>,
}

pub fn list_displays() -> Vec<DisplayInfo> {
    let mut out = Vec::new();
    if cfg!(target_os = "macos") {
        let text = run_text("osascript", &["-l", "JavaScript", "-e", MAC_DISPLAYS_JS]);
        let rows: Vec<Vec<String>> = text.lines().map(|l| l.split('\t').map(|x| x.trim().to_string()).collect()).filter(|f: &Vec<String>| f.len() >= 6).collect();
        let num = |s: &str| s.parse::<f32>().unwrap_or(0.0);
        let main_h = rows.first().map(|f| num(&f[5])).unwrap_or(0.0);
        for (i, f) in rows.iter().enumerate() {
            let (x, y, w, h) = (num(&f[2]), num(&f[3]), num(&f[4]), num(&f[5]));
            out.push(DisplayInfo { name: format!("Display {} · {} ({})", i + 1, f[0], f[1]), rect: Some([x, main_h - (y + h), w, h]) });
        }
    } else if cfg!(windows) {
        let text = run_text(
            "powershell",
            &["-NoProfile", "-Command", "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Screen]::AllScreens | ForEach-Object { \"$($_.Bounds.X),$($_.Bounds.Y),$($_.Bounds.Width),$($_.Bounds.Height),$($_.Primary)\" }"],
        );
        for (i, line) in text.lines().enumerate() {
            let f: Vec<f32> = line.trim().split(',').take(4).filter_map(|v| v.trim().parse().ok()).collect();
            if f.len() == 4 {
                let main = if line.trim().ends_with("True") { ", main" } else { "" };
                out.push(DisplayInfo { name: format!("Display {} ({}x{}{})", i + 1, f[2], f[3], main), rect: Some([f[0], f[1], f[2], f[3]]) });
            }
        }
    } else {
        for (i, line) in run_text("xrandr", &["--listmonitors"]).lines().skip(1).enumerate() {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 4 {
                continue;
            }
            let rect = (|| -> Option<[f32; 4]> {
                let (wpart, rest) = f[2].split_once('x')?;
                let w: f32 = wpart.split('/').next()?.parse().ok()?;
                let mut it = rest.split('+');
                let h: f32 = it.next()?.split('/').next()?.parse().ok()?;
                Some([it.next()?.parse().ok()?, it.next()?.parse().ok()?, w, h])
            })();
            out.push(DisplayInfo { name: format!("Display {} · {}", i + 1, f[3]), rect });
        }
    }
    if out.is_empty() {
        out = (1..=3).map(|i| DisplayInfo { name: format!("Display {i}"), rect: None }).collect();
    }
    out
}

pub fn list_capture_targets(ffmpeg: &Path) -> (Vec<CaptureTarget>, String) {
    if cfg!(target_os = "macos") {
        mac_targets(ffmpeg)
    } else if cfg!(windows) {
        windows_targets()
    } else {
        linux_targets()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn lists_displays() {
        let d = super::list_displays();
        println!("{d:?}");
        assert!(!d.is_empty());
    }
}

#[cfg(test)]
mod args_tests {
    #[test]
    #[ignore]
    fn shows_mac_args() {
        let ff = crate::media::locate_ffmpeg("").unwrap().path;
        crate::media::refresh_mac_devices(&ff);
        for spec in ["screen:@0|Screen 1", "window:@0,100,100,800,600,1920,1080|W", "0|Brio 500"] {
            println!("{spec} => {:?}", super::input_args(spec, 0));
        }
        println!("{:?}", crate::media::mac_window_base("c:window:@0,100,100,800,600,1920,1080|W"));
    }
}
