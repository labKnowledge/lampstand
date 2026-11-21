use crate::audio::{AudioEngine, AudioSink};
use crate::media::ffmpeg_command;
use crate::model::{now, Destination, Quality};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const FPS: u32 = 30;

#[derive(Clone, Debug, PartialEq)]
pub enum SinkState {
    Starting,
    Running,
    Stopping,
    Ended,
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct SinkStats {
    pub state: SinkState,
    pub fps: f32,
    pub kbps: f32,
    pub speed: f32,
    pub frames: u64,
    pub log: VecDeque<String>,
    pub failed: Vec<usize>,
    pub started_at: f64,
    pub paused_total: f64,
    pub paused_since: Option<f64>,
}

impl SinkStats {
    pub fn elapsed(&self, t: f64) -> f64 {
        let paused_now = self.paused_since.map(|p| t - p).unwrap_or(0.0);
        (t - self.started_at - self.paused_total - paused_now).max(0.0)
    }
}

pub struct Sink {
    pub id: u64,
    pub label: String,
    pub dest_names: Vec<String>,
    pub path: Option<PathBuf>,
    pub stats: Arc<Mutex<SinkStats>>,
    pub ready: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
    pub dropped_video: Arc<AtomicU64>,
    pub dropped_audio: Arc<AtomicU64>,
    video_tx: Mutex<Option<SyncSender<Arc<Vec<u8>>>>>,
    video_clock: Mutex<VideoClock>,
    child: Arc<Mutex<Option<Child>>>,
}

#[derive(Default)]
struct VideoClock {
    started: Option<f64>,
    sent: u64,
}

const MAX_OWED_FRAMES: u64 = FPS as u64 * 10;

impl Sink {
    pub fn push_video(&self, frame: &Arc<Vec<u8>>) {
        if !self.ready.load(Ordering::SeqCst) || self.paused.load(Ordering::SeqCst) {
            return;
        }
        let t = now();
        let paused_total = self.stats.lock().unwrap().paused_total;
        let tx = self.video_tx.lock().unwrap();
        let Some(tx) = tx.as_ref() else { return };
        let mut clock = self.video_clock.lock().unwrap();
        let started = *clock.started.get_or_insert(t);
        let due = ((t - started - paused_total).max(0.0) * FPS as f64) as u64 + 1;
        let mut owed = due.saturating_sub(clock.sent);
        if owed > MAX_OWED_FRAMES {
            let lost = owed - MAX_OWED_FRAMES;
            self.dropped_video.fetch_add(lost, Ordering::Relaxed);
            clock.sent += lost;
            owed = MAX_OWED_FRAMES;
        }
        while owed > 0 && tx.try_send(frame.clone()).is_ok() {
            clock.sent += 1;
            owed -= 1;
        }
    }

    pub fn set_paused(&self, p: bool) {
        let was = self.paused.swap(p, Ordering::SeqCst);
        let t = now();
        let mut s = self.stats.lock().unwrap();
        if p && !was {
            s.paused_since = Some(t);
        } else if !p && was {
            if let Some(since) = s.paused_since.take() {
                s.paused_total += t - since;
            }
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self.stats.lock().unwrap().state, SinkState::Starting | SinkState::Running)
    }

    pub fn stop(&self, audio: &AudioEngine) {
        {
            let mut s = self.stats.lock().unwrap();
            if matches!(s.state, SinkState::Starting | SinkState::Running) {
                s.state = SinkState::Stopping;
            }
        }
        self.ready.store(false, Ordering::SeqCst);
        audio.remove_sink(self.id);
        self.video_tx.lock().unwrap().take();
        let child = self.child.clone();
        let stats = self.stats.clone();
        std::thread::spawn(move || {
            for _ in 0..100 {
                std::thread::sleep(Duration::from_millis(100));
                let mut g = child.lock().unwrap();
                match g.as_mut().map(|c| c.try_wait()) {
                    Some(Ok(Some(_))) | None => {
                        g.take();
                        let mut s = stats.lock().unwrap();
                        if s.state == SinkState::Stopping {
                            s.state = SinkState::Ended;
                        }
                        return;
                    }
                    _ => {}
                }
            }
            if let Some(mut c) = child.lock().unwrap().take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            let mut s = stats.lock().unwrap();
            if s.state == SinkState::Stopping {
                s.state = SinkState::Ended;
            }
        });
    }
}

pub fn video_args(q: Quality, kbps: u32, encoder: &str) -> Vec<String> {
    let gop = (FPS * 2).to_string();
    let b = format!("{kbps}k");
    let buf = format!("{}k", kbps * 2);
    let mut a: Vec<String> = vec!["-c:v".into(), encoder.into()];
    match encoder {
        "libx264" => a.extend(["-preset", "veryfast", "-profile:v", "high", "-sc_threshold", "0"].map(String::from)),
        "h264_videotoolbox" => a.extend(["-realtime", "1", "-allow_sw", "1", "-profile:v", "high"].map(String::from)),
        "h264_nvenc" => a.extend(["-preset", "p4", "-tune", "ll", "-rc", "cbr"].map(String::from)),
        "h264_qsv" => a.extend(["-preset", "veryfast"].map(String::from)),
        "h264_amf" => a.extend(["-usage", "lowlatency", "-rc", "cbr"].map(String::from)),
        _ => {}
    }
    let (w, h) = q.size();
    a.extend([
        "-s".into(),
        format!("{w}x{h}"),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-r".into(),
        FPS.to_string(),
        "-g".into(),
        gop.clone(),
        "-keyint_min".into(),
        gop,
        "-b:v".into(),
        b.clone(),
        "-maxrate".into(),
        b,
        "-bufsize".into(),
        buf,
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "160k".into(),
        "-ar".into(),
        "48000".into(),
        "-ac".into(),
        "2".into(),
    ]);
    a
}

fn tee_escape(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        if matches!(c, '|' | '[' | ']' | '\\' | '\'') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

pub fn live_output_args(dests: &[Destination]) -> Vec<String> {
    let urls: Vec<String> = dests.iter().map(|d| d.url()).collect();
    if urls.len() == 1 {
        return vec!["-f".into(), "flv".into(), urls[0].clone()];
    }
    let spec = urls.iter().map(|u| format!("[f=flv:onfail=ignore]{}", tee_escape(u))).collect::<Vec<_>>().join("|");
    vec!["-flags".into(), "+global_header".into(), "-f".into(), "tee".into(), spec]
}

pub fn record_output_args(path: &Path) -> Vec<String> {
    vec!["-movflags".into(), "+frag_keyframe+empty_moov+default_base_moof".into(), "-f".into(), "mp4".into(), path.to_string_lossy().to_string()]
}

pub struct StartSpec<'a> {
    pub ffmpeg: &'a Path,
    pub canvas: (u32, u32),
    pub quality: Quality,
    pub kbps: u32,
    pub encoder: &'a str,
    pub output: Vec<String>,
    pub label: String,
    pub dest_names: Vec<String>,
    pub secrets: Vec<String>,
    pub path: Option<PathBuf>,
    pub stats_period: bool,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn start_sink(spec: StartSpec, audio: &AudioEngine) -> Result<Arc<Sink>, String> {
    let port = TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map_err(|e| e.to_string())?.port();
    let (cw, ch) = spec.canvas;
    let mut cmd = ffmpeg_command(spec.ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "warning", "-nostats", "-progress", "pipe:1"]);
    if spec.stats_period {
        cmd.args(["-stats_period", "1"]);
    }
    cmd.args(["-thread_queue_size", "1024", "-probesize", "32", "-analyzeduration", "0", "-f", "f32le", "-ar", "48000", "-ac", "2"]);
    cmd.arg("-i").arg(format!("tcp://127.0.0.1:{port}?listen=1&listen_timeout=15000"));
    cmd.args(["-thread_queue_size", "64", "-probesize", "32", "-analyzeduration", "0", "-f", "rawvideo", "-pix_fmt", "rgba"]);
    cmd.arg("-s").arg(format!("{cw}x{ch}")).arg("-framerate").arg(FPS.to_string()).args(["-i", "pipe:0"]);
    cmd.args(["-map", "1:v", "-map", "0:a"]);
    cmd.args(video_args(spec.quality, spec.kbps, spec.encoder));
    cmd.args(&spec.output);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Could not start FFmpeg: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let stderr = child.stderr.take().ok_or("no stderr")?;
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let stats = Arc::new(Mutex::new(SinkStats {
        state: SinkState::Starting,
        fps: 0.0,
        kbps: 0.0,
        speed: 0.0,
        frames: 0,
        log: VecDeque::new(),
        failed: vec![],
        started_at: now(),
        paused_total: 0.0,
        paused_since: None,
    }));
    let ready = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    let (vtx, vrx) = mpsc::sync_channel::<Arc<Vec<u8>>>(45);
    let (atx, arx) = mpsc::sync_channel::<Arc<Vec<f32>>>(400);
    let child = Arc::new(Mutex::new(Some(child)));
    let dropped_audio = Arc::new(AtomicU64::new(0));
    audio.add_sink(AudioSink { id, tx: atx, ready: ready.clone(), paused: paused.clone(), dropped: dropped_audio.clone(), owed: 0 });

    {
        let ready = ready.clone();
        let stats = stats.clone();
        std::thread::spawn(move || {
            let mut conn = None;
            for _ in 0..320 {
                if !matches!(stats.lock().unwrap().state, SinkState::Starting) {
                    return;
                }
                if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
                    conn = Some(s);
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let Some(mut conn) = conn else {
                let mut s = stats.lock().unwrap();
                if s.state == SinkState::Starting {
                    s.state = SinkState::Failed("The encoder did not open its sound input".into());
                }
                return;
            };
            let _ = conn.set_nodelay(true);
            ready.store(true, Ordering::SeqCst);
            let mut bytes = Vec::with_capacity(4096);
            while let Ok(block) = arx.recv() {
                bytes.clear();
                for v in block.iter() {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
                if conn.write_all(&bytes).is_err() {
                    break;
                }
            }
        });
    }
    std::thread::spawn(move || {
        while let Ok(frame) = vrx.recv() {
            if stdin.write_all(&frame).is_err() {
                break;
            }
        }
    });
    {
        let stats = stats.clone();
        std::thread::spawn(move || {
            let mut kv = BufReader::new(stdout);
            let mut line = String::new();
            while kv.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
                if let Some((k, v)) = line.trim().split_once('=') {
                    let mut s = stats.lock().unwrap();
                    match k {
                        "frame" => {
                            s.frames = v.trim().parse().unwrap_or(s.frames);
                            if s.frames > 0 && s.state == SinkState::Starting {
                                s.state = SinkState::Running;
                            }
                        }
                        "fps" => s.fps = v.trim().parse().unwrap_or(0.0),
                        "bitrate" => s.kbps = v.trim().trim_end_matches("kbits/s").trim().parse().unwrap_or(0.0),
                        "speed" => s.speed = v.trim().trim_end_matches('x').trim().parse().unwrap_or(0.0),
                        _ => {}
                    }
                }
                line.clear();
            }
        });
    }
    {
        let stats = stats.clone();
        let secrets = spec.secrets.clone();
        let child = child.clone();
        std::thread::spawn(move || {
            let mut r = BufReader::new(stderr);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                let mut byte = [0u8; 1];
                let mut ended = false;
                loop {
                    match r.read(&mut byte) {
                        Ok(0) | Err(_) => {
                            ended = true;
                            break;
                        }
                        Ok(_) => {
                            if byte[0] == b'\n' || byte[0] == b'\r' {
                                break;
                            }
                            buf.push(byte[0]);
                        }
                    }
                }
                let mut l = String::from_utf8_lossy(&buf).trim().to_string();
                if !l.is_empty() {
                    for sec in &secrets {
                        if sec.len() > 3 {
                            l = l.replace(sec.as_str(), "••••");
                        }
                    }
                    let mut s = stats.lock().unwrap();
                    if let Some(i) = l.find("Slave muxer #") {
                        if let Some(n) = l[i + 13..].split(|c: char| !c.is_ascii_digit()).next().and_then(|n| n.parse::<usize>().ok()) {
                            if !s.failed.contains(&n) {
                                s.failed.push(n);
                            }
                        }
                    }
                    s.log.push_back(l);
                    while s.log.len() > 60 {
                        s.log.pop_front();
                    }
                }
                if ended {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(300));
            let code = child.lock().unwrap().as_mut().and_then(|c| c.try_wait().ok().flatten()).map(|s| s.success());
            let mut s = stats.lock().unwrap();
            if matches!(s.state, SinkState::Starting | SinkState::Running) {
                let last = s.log.iter().rev().find(|l| !l.is_empty()).cloned().unwrap_or_else(|| "The encoder stopped".into());
                s.state = if code == Some(true) { SinkState::Ended } else { SinkState::Failed(last) };
            }
        });
    }
    Ok(Arc::new(Sink {
        id,
        label: spec.label,
        dest_names: spec.dest_names,
        path: spec.path,
        stats,
        ready,
        paused,
        dropped_video: Arc::new(AtomicU64::new(0)),
        dropped_audio,
        video_tx: Mutex::new(Some(vtx)),
        video_clock: Mutex::new(VideoClock::default()),
        child,
    }))
}

pub fn check_destination(url: &str) -> Result<String, String> {
    let u = url.trim();
    let (scheme, rest) = u.split_once("://").ok_or("The server address should start with rtmp:// or rtmps://")?;
    let default_port = match scheme.to_lowercase().as_str() {
        "rtmp" => 1935,
        "rtmps" => 443,
        "srt" => return Ok("SRT addresses are used as typed".into()),
        _ => return Err("The server address should start with rtmp:// or rtmps://".into()),
    };
    let hostport = rest.split('/').next().unwrap_or("");
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (h.to_string(), p.parse().unwrap_or(default_port)),
        _ => (hostport.to_string(), default_port),
    };
    if host.is_empty() {
        return Err("The server address is missing a host name".into());
    }
    let addrs: Vec<_> = (host.as_str(), port).to_socket_addrs().map_err(|e| format!("Could not find {host}: {e}"))?.collect();
    let addr = addrs.first().ok_or(format!("Could not find {host}"))?;
    let t0 = std::time::Instant::now();
    TcpStream::connect_timeout(addr, Duration::from_secs(5)).map_err(|e| format!("Could not reach {host}:{port}: {e}"))?;
    Ok(format!("Reached {host} in {} ms", t0.elapsed().as_millis()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tee_spec_for_two_destinations() {
        let d = vec![
            Destination { service: "youtube".into(), name: "YouTube".into(), server: "rtmp://a.rtmp.youtube.com/live2".into(), key: "abcd-1234".into(), enabled: true },
            Destination { service: "facebook".into(), name: "Facebook".into(), server: "rtmps://live-api-s.facebook.com:443/rtmp/".into(), key: "FB-1|x".into(), enabled: true },
        ];
        let a = live_output_args(&d);
        let spec = a.last().unwrap();
        assert!(spec.contains("[f=flv:onfail=ignore]rtmp://a.rtmp.youtube.com/live2/abcd-1234"));
        assert!(spec.contains("rtmp/FB-1\\|x"));
        let one = live_output_args(&d[..1]);
        assert_eq!(one, vec!["-f", "flv", "rtmp://a.rtmp.youtube.com/live2/abcd-1234"]);
    }
}

#[cfg(test)]
mod reach_tests {
    use super::*;
    use crate::services::SERVICES;

    #[test]
    #[ignore]
    fn reaches_platform_servers() {
        for s in SERVICES.iter().filter(|s| s.server.contains("://") && s.server.len() > 8) {
            println!("{}: {:?}", s.name, check_destination(s.server));
        }
    }
}
