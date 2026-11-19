use crate::media::ffmpeg_command;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

pub const RATE: u32 = 48_000;
pub const BLOCK: usize = 480;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SrcKind {
    Music,
    Media,
}

#[derive(Clone, Copy, Debug)]
pub struct Controls {
    pub mic_on: bool,
    pub mic_gain: f32,
    pub music_gain: f32,
    pub media_gain: f32,
    pub master: f32,
    pub monitor_on: bool,
    pub break_mute_mic: bool,
}

impl Default for Controls {
    fn default() -> Self {
        Controls { mic_on: true, mic_gain: 1.0, music_gain: 0.7, media_gain: 0.9, master: 1.0, monitor_on: true, break_mute_mic: false }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Meters {
    pub mic: f32,
    pub music: f32,
    pub media: f32,
    pub program: f32,
}

struct Source {
    kind: SrcKind,
    rx: Receiver<Vec<f32>>,
    buf: VecDeque<f32>,
    gain: f32,
    target: f32,
    step: f32,
    reader_done: bool,
    remove_when_silent: bool,
    child: Arc<Mutex<Option<Child>>>,
    stop: Arc<AtomicBool>,
    played: u64,
}

pub struct AudioSink {
    pub id: u64,
    pub tx: SyncSender<Arc<Vec<f32>>>,
    pub ready: Arc<AtomicBool>,
    pub paused: Arc<AtomicBool>,
    pub dropped: Arc<std::sync::atomic::AtomicU64>,
}

enum DeviceCmd {
    Input(Option<String>),
    Output(Option<String>),
}

pub struct AudioEngine {
    pub ffmpeg: Arc<RwLock<Option<PathBuf>>>,
    pub controls: Mutex<Controls>,
    pub meters: Mutex<Meters>,
    sources: Mutex<HashMap<String, Source>>,
    finished: Mutex<Vec<String>>,
    mic_buf: Arc<Mutex<VecDeque<f32>>>,
    monitor_buf: Arc<Mutex<VecDeque<f32>>>,
    pub sinks: Mutex<Vec<AudioSink>>,
    device_tx: Mutex<Option<Sender<DeviceCmd>>>,
    pub mic_status: Arc<Mutex<String>>,
    pub out_status: Arc<Mutex<String>>,
}

impl AudioEngine {
    pub fn new(ffmpeg: Arc<RwLock<Option<PathBuf>>>) -> Arc<Self> {
        Arc::new(AudioEngine {
            ffmpeg,
            controls: Mutex::new(Controls::default()),
            meters: Mutex::new(Meters::default()),
            sources: Mutex::new(HashMap::new()),
            finished: Mutex::new(Vec::new()),
            mic_buf: Arc::new(Mutex::new(VecDeque::new())),
            monitor_buf: Arc::new(Mutex::new(VecDeque::new())),
            sinks: Mutex::new(Vec::new()),
            device_tx: Mutex::new(None),
            mic_status: Arc::new(Mutex::new("Microphone off".into())),
            out_status: Arc::new(Mutex::new("Speakers not started".into())),
        })
    }

    pub fn start_mixer(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::Builder::new().name("mixer".into()).spawn(move || me.mixer_loop()).ok();
    }

    pub fn start(self: &Arc<Self>, output_device: Option<String>) {
        self.start_mixer();
        let (tx, rx) = mpsc::channel();
        *self.device_tx.lock().unwrap() = Some(tx.clone());
        let mic_buf = self.mic_buf.clone();
        let mon = self.monitor_buf.clone();
        let mic_status = self.mic_status.clone();
        let out_status = self.out_status.clone();
        std::thread::Builder::new()
            .name("audio-devices".into())
            .spawn(move || device_loop(rx, mic_buf, mon, mic_status, out_status))
            .ok();
        let _ = tx.send(DeviceCmd::Output(output_device));
    }

    pub fn set_input(&self, name: Option<String>) {
        if let Some(tx) = self.device_tx.lock().unwrap().as_ref() {
            let _ = tx.send(DeviceCmd::Input(name));
        }
    }

    pub fn set_output(&self, name: Option<String>) {
        if let Some(tx) = self.device_tx.lock().unwrap().as_ref() {
            let _ = tx.send(DeviceCmd::Output(name));
        }
    }

    pub fn play(&self, key: &str, path: &str, kind: SrcKind, looping: bool, gain: f32, fade_in: f32) {
        self.kill(key);
        let Some(ff) = self.ffmpeg.read().unwrap().clone() else { return };
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(6);
        let child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let mut cmd = ffmpeg_command(&ff);
        cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
        if looping {
            cmd.args(["-stream_loop", "-1"]);
        }
        cmd.arg("-i").arg(path).args(["-vn", "-f", "f32le", "-ac", "2", "-ar", "48000", "pipe:1"]);
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        if let Ok(mut c) = cmd.spawn() {
            let mut out = c.stdout.take().unwrap();
            *child.lock().unwrap() = Some(c);
            let stop2 = stop.clone();
            std::thread::spawn(move || {
                let mut bytes = vec![0u8; 4800 * 2 * 4];
                loop {
                    if stop2.load(Ordering::SeqCst) {
                        break;
                    }
                    let mut filled = 0;
                    while filled < bytes.len() {
                        match out.read(&mut bytes[filled..]) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => filled += n,
                        }
                    }
                    let usable = filled / 4 * 4;
                    if usable == 0 {
                        break;
                    }
                    let samples: Vec<f32> = bytes[..usable].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                    if tx.send(samples).is_err() {
                        break;
                    }
                    if filled < bytes.len() {
                        break;
                    }
                }
            });
        }
        let fade = fade_in.max(0.01);
        let src = Source {
            kind,
            rx,
            buf: VecDeque::new(),
            gain: if fade_in > 0.0 { 0.0 } else { gain },
            target: gain,
            step: 1.0 / (RATE as f32 * fade),
            reader_done: false,
            remove_when_silent: false,
            child,
            stop,
            played: 0,
        };
        self.sources.lock().unwrap().insert(key.to_string(), src);
    }

    fn kill(&self, key: &str) {
        if let Some(s) = self.sources.lock().unwrap().remove(key) {
            s.stop.store(true, Ordering::SeqCst);
            if let Some(mut c) = s.child.lock().unwrap().take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }

    pub fn stop(&self, key: &str, fade: f32) {
        let mut s = self.sources.lock().unwrap();
        if let Some(src) = s.get_mut(key) {
            src.target = 0.0;
            src.step = 1.0 / (RATE as f32 * fade.max(0.01));
            src.remove_when_silent = true;
        }
    }

    pub fn is_playing(&self, key: &str) -> bool {
        self.sources.lock().unwrap().get(key).map(|s| !s.remove_when_silent).unwrap_or(false)
    }

    pub fn position(&self, key: &str) -> Option<f64> {
        self.sources.lock().unwrap().get(key).map(|s| s.played as f64 / RATE as f64)
    }

    pub fn take_finished(&self) -> Vec<String> {
        std::mem::take(&mut *self.finished.lock().unwrap())
    }

    pub fn add_sink(&self, sink: AudioSink) {
        self.sinks.lock().unwrap().push(sink);
    }

    pub fn remove_sink(&self, id: u64) {
        self.sinks.lock().unwrap().retain(|s| s.id != id);
    }

    fn mixer_loop(self: Arc<Self>) {
        let start = Instant::now();
        let mut produced: u64 = 0;
        let mut prog = vec![0f32; BLOCK * 2];
        let mut mon = vec![0f32; BLOCK * 2];
        let mut peaks = Meters::default();
        loop {
            std::thread::sleep(Duration::from_millis(4));
            let due = (start.elapsed().as_secs_f64() * RATE as f64) as u64;
            if due.saturating_sub(produced) > RATE as u64 {
                produced = due - BLOCK as u64;
            }
            while produced + BLOCK as u64 <= due {
                produced += BLOCK as u64;
                prog.iter_mut().for_each(|x| *x = 0.0);
                mon.iter_mut().for_each(|x| *x = 0.0);
                let ctl = *self.controls.lock().unwrap();
                let mut pk = Meters::default();
                {
                    let mut mb = self.mic_buf.lock().unwrap();
                    let live_mic = ctl.mic_on && !ctl.break_mute_mic;
                    for i in 0..BLOCK * 2 {
                        let v = mb.pop_front().unwrap_or(0.0) * ctl.mic_gain;
                        pk.mic = pk.mic.max(v.abs());
                        if live_mic {
                            prog[i] += v;
                        }
                    }
                    if mb.len() > RATE as usize / 2 {
                        let extra = mb.len() - RATE as usize / 4;
                        mb.drain(..extra);
                    }
                }
                {
                    let mut srcs = self.sources.lock().unwrap();
                    let mut done = Vec::new();
                    for (key, s) in srcs.iter_mut() {
                        while s.buf.len() < BLOCK * 2 && !s.reader_done {
                            match s.rx.try_recv() {
                                Ok(v) => s.buf.extend(v),
                                Err(mpsc::TryRecvError::Empty) => break,
                                Err(mpsc::TryRecvError::Disconnected) => s.reader_done = true,
                            }
                        }
                        let kg = match s.kind {
                            SrcKind::Music => ctl.music_gain,
                            SrcKind::Media => ctl.media_gain,
                        };
                        let have = s.buf.len().min(BLOCK * 2);
                        for i in 0..have {
                            if i % 2 == 0 {
                                if s.gain < s.target {
                                    s.gain = (s.gain + s.step).min(s.target);
                                } else if s.gain > s.target {
                                    s.gain = (s.gain - s.step).max(s.target);
                                }
                            }
                            let v = s.buf.pop_front().unwrap_or(0.0) * s.gain * kg;
                            prog[i] += v;
                            mon[i] += v;
                            match s.kind {
                                SrcKind::Music => pk.music = pk.music.max(v.abs()),
                                SrcKind::Media => pk.media = pk.media.max(v.abs()),
                            }
                        }
                        s.played += (have / 2) as u64;
                        if (s.reader_done && s.buf.is_empty()) || (s.remove_when_silent && s.gain <= 0.0005) {
                            done.push(key.clone());
                        }
                    }
                    for k in done {
                        if let Some(s) = srcs.remove(&k) {
                            s.stop.store(true, Ordering::SeqCst);
                            if let Some(mut c) = s.child.lock().unwrap().take() {
                                let _ = c.kill();
                                let _ = c.wait();
                            }
                            if !s.remove_when_silent {
                                self.finished.lock().unwrap().push(k);
                            }
                        }
                    }
                }
                for v in prog.iter_mut() {
                    *v = soft_clip(*v * ctl.master);
                    pk.program = pk.program.max(v.abs());
                }
                if ctl.monitor_on {
                    let mut m = self.monitor_buf.lock().unwrap();
                    m.extend(mon.iter().map(|v| soft_clip(*v)));
                    let cap = RATE as usize / 2;
                    if m.len() > cap {
                        let extra = m.len() - cap / 2;
                        m.drain(..extra);
                    }
                }
                let block = Arc::new(prog.clone());
                for s in self.sinks.lock().unwrap().iter() {
                    if s.ready.load(Ordering::SeqCst) && !s.paused.load(Ordering::SeqCst) && s.tx.try_send(block.clone()).is_err() {
                        s.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
                peaks.mic = (peaks.mic * 0.93).max(pk.mic);
                peaks.music = (peaks.music * 0.93).max(pk.music);
                peaks.media = (peaks.media * 0.93).max(pk.media);
                peaks.program = (peaks.program * 0.93).max(pk.program);
            }
            *self.meters.lock().unwrap() = peaks;
        }
    }
}

fn soft_clip(v: f32) -> f32 {
    if v.abs() <= 0.9 { v } else { v.signum() * (0.9 + 0.1 * ((v.abs() - 0.9) * 10.0).tanh()) }
}

pub fn device_names(input: bool) -> Vec<String> {
    let host = cpal::default_host();
    let devs = if input { host.input_devices().map(|d| d.collect::<Vec<_>>()) } else { host.output_devices().map(|d| d.collect::<Vec<_>>()) };
    devs.unwrap_or_default().iter().filter_map(|d| d.description().ok().map(|x| x.name().to_string())).collect()
}

fn find_device(input: bool, name: &Option<String>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if let Some(n) = name.as_ref().filter(|n| !n.is_empty()) {
        let devs = if input { host.input_devices().ok().map(|d| d.collect::<Vec<_>>()) } else { host.output_devices().ok().map(|d| d.collect::<Vec<_>>()) };
        if let Some(d) = devs.unwrap_or_default().into_iter().find(|d| d.description().map(|x| x.name() == n).unwrap_or(false)) {
            return Some(d);
        }
    }
    if input { host.default_input_device() } else { host.default_output_device() }
}

struct Resample {
    step: f64,
    pos: f64,
    hold: VecDeque<[f32; 2]>,
}

impl Resample {
    fn new(from: u32, to: u32) -> Self {
        Resample { step: from as f64 / to as f64, pos: 0.0, hold: VecDeque::new() }
    }
    fn push(&mut self, f: [f32; 2]) {
        self.hold.push_back(f);
    }
    fn next(&mut self) -> Option<[f32; 2]> {
        let i = self.pos.floor() as usize;
        if i + 1 >= self.hold.len() {
            return None;
        }
        let t = (self.pos - i as f64) as f32;
        let a = self.hold[i];
        let b = self.hold[i + 1];
        self.pos += self.step;
        let drop = self.pos.floor() as usize;
        if drop > 0 {
            let d = drop.min(self.hold.len());
            self.hold.drain(..d);
            self.pos -= d as f64;
        }
        Some([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
    }
}

fn build_input<T>(dev: &cpal::Device, cfg: cpal::StreamConfig, buf: Arc<Mutex<VecDeque<f32>>>) -> Result<cpal::Stream, String>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let ch = cfg.channels as usize;
    let mut rs = Resample::new(cfg.sample_rate, RATE);
    dev.build_input_stream::<T, _, _>(
        cfg,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            for frame in data.chunks(ch.max(1)) {
                let l = f32::from_sample(frame[0]);
                let r = if ch > 1 { f32::from_sample(frame[1]) } else { l };
                rs.push([l, r]);
            }
            let mut b = buf.lock().unwrap();
            while let Some(f) = rs.next() {
                b.push_back(f[0]);
                b.push_back(f[1]);
            }
        },
        |_e| {},
        None,
    )
    .map_err(|e| e.to_string())
}

fn build_output<T>(dev: &cpal::Device, cfg: cpal::StreamConfig, buf: Arc<Mutex<VecDeque<f32>>>) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let ch = cfg.channels as usize;
    let mut rs = Resample::new(RATE, cfg.sample_rate);
    dev.build_output_stream::<T, _, _>(
        cfg,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / ch.max(1);
            {
                let mut b = buf.lock().unwrap();
                let need = (frames as f64 * rs.step).ceil() as usize + 2;
                while rs.hold.len() < need {
                    match (b.pop_front(), b.pop_front()) {
                        (Some(l), Some(r)) => rs.push([l, r]),
                        _ => break,
                    }
                }
            }
            for frame in data.chunks_mut(ch.max(1)) {
                let f = rs.next().unwrap_or([0.0, 0.0]);
                for (i, s) in frame.iter_mut().enumerate() {
                    let v = if ch == 1 { (f[0] + f[1]) * 0.5 } else if i == 0 { f[0] } else if i == 1 { f[1] } else { 0.0 };
                    *s = T::from_sample(v);
                }
            }
        },
        |_e| {},
        None,
    )
    .map_err(|e| e.to_string())
}

fn open_input(name: &Option<String>, buf: Arc<Mutex<VecDeque<f32>>>) -> Result<(cpal::Stream, String), String> {
    let dev = find_device(true, name).ok_or("No microphone found")?;
    let label = dev.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Microphone".into());
    let sc = dev.default_input_config().map_err(|e| e.to_string())?;
    let cfg: cpal::StreamConfig = sc.config();
    let stream = match sc.sample_format() {
        cpal::SampleFormat::F32 => build_input::<f32>(&dev, cfg, buf),
        cpal::SampleFormat::I16 => build_input::<i16>(&dev, cfg, buf),
        cpal::SampleFormat::U16 => build_input::<u16>(&dev, cfg, buf),
        cpal::SampleFormat::I32 => build_input::<i32>(&dev, cfg, buf),
        f => Err(format!("Unsupported microphone format {f:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, label))
}

fn open_output(name: &Option<String>, buf: Arc<Mutex<VecDeque<f32>>>) -> Result<(cpal::Stream, String), String> {
    let dev = find_device(false, name).ok_or("No speakers found")?;
    let label = dev.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Speakers".into());
    let sc = dev.default_output_config().map_err(|e| e.to_string())?;
    let cfg: cpal::StreamConfig = sc.config();
    let stream = match sc.sample_format() {
        cpal::SampleFormat::F32 => build_output::<f32>(&dev, cfg, buf),
        cpal::SampleFormat::I16 => build_output::<i16>(&dev, cfg, buf),
        cpal::SampleFormat::U16 => build_output::<u16>(&dev, cfg, buf),
        cpal::SampleFormat::I32 => build_output::<i32>(&dev, cfg, buf),
        f => Err(format!("Unsupported speaker format {f:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, label))
}

fn device_loop(rx: Receiver<DeviceCmd>, mic_buf: Arc<Mutex<VecDeque<f32>>>, mon: Arc<Mutex<VecDeque<f32>>>, mic_status: Arc<Mutex<String>>, out_status: Arc<Mutex<String>>) {
    let mut input: Option<cpal::Stream> = None;
    let mut output: Option<cpal::Stream> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            DeviceCmd::Input(name) => {
                input = None;
                mic_buf.lock().unwrap().clear();
                if let Some(n) = name {
                    match open_input(&Some(n), mic_buf.clone()) {
                        Ok((s, label)) => {
                            input = Some(s);
                            *mic_status.lock().unwrap() = format!("Listening: {label}");
                        }
                        Err(e) => *mic_status.lock().unwrap() = format!("Microphone problem: {e}"),
                    }
                } else {
                    *mic_status.lock().unwrap() = "Microphone off".into();
                }
            }
            DeviceCmd::Output(name) => {
                output = None;
                match open_output(&name, mon.clone()) {
                    Ok((s, label)) => {
                        output = Some(s);
                        *out_status.lock().unwrap() = format!("Room sound: {label}");
                    }
                    Err(e) => *out_status.lock().unwrap() = format!("Speaker problem: {e}"),
                }
            }
        }
        let _ = (&input, &output);
    }
}
