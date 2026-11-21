use crate::audio::AudioEngine;
use crate::canvas::Canvas;
use crate::media::{ImageStore, VideoHub};
use crate::model::{now, LiveState, OutputId};
use crate::render::Renderer;
use crate::stream::{Sink, FPS};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

pub struct Frames {
    pub w: u32,
    pub h: u32,
    pub screen: Arc<Vec<u8>>,
    pub stream: Arc<Vec<u8>>,
    pub seq: u64,
}

#[derive(Clone, Copy, Default)]
pub struct EngineStats {
    pub fps: f32,
    pub render_ms: f32,
}

pub struct Shared {
    pub state: Mutex<LiveState>,
    pub frames: Mutex<Frames>,
    pub sinks: Mutex<Vec<Arc<Sink>>>,
    pub videos: VideoHub,
    pub images: ImageStore,
    pub audio: Arc<AudioEngine>,
    pub ffmpeg: Arc<RwLock<Option<PathBuf>>>,
    pub ctx: Mutex<Option<eframe::egui::Context>>,
    pub stats: Mutex<EngineStats>,
    pub preview: Mutex<std::collections::HashMap<String, bool>>,
}

impl Shared {
    pub fn new(state: LiveState, ffmpeg: Option<PathBuf>) -> Arc<Self> {
        let ff = Arc::new(RwLock::new(ffmpeg));
        let audio = AudioEngine::new(ff.clone());
        let (w, h) = state.canvas;
        Arc::new(Shared {
            state: Mutex::new(state),
            frames: Mutex::new(Frames { w, h, screen: Arc::new(vec![0; (w * h * 4) as usize]), stream: Arc::new(vec![0; (w * h * 4) as usize]), seq: 0 }),
            sinks: Mutex::new(Vec::new()),
            videos: VideoHub::new(ff.clone(), audio.clone()),
            images: ImageStore::new(),
            audio,
            ffmpeg: ff,
            ctx: Mutex::new(None),
            stats: Mutex::new(EngineStats::default()),
            preview: Mutex::new(std::collections::HashMap::new()),
        })
    }

    pub fn edit<R>(&self, f: impl FnOnce(&mut LiveState) -> R) -> R {
        let mut s = self.state.lock().unwrap();
        let r = f(&mut s);
        s.touch();
        r
    }

    pub fn snapshot(&self) -> LiveState {
        self.state.lock().unwrap().clone()
    }

    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::Builder::new().name("compositor".into()).spawn(move || me.run()).ok();
    }

    pub fn render_once(&self, renderer: &mut Renderer, canvases: &mut Option<(Canvas, Canvas)>) -> bool {
        let st = self.snapshot();
        let (w, h) = st.canvas;
        if canvases.as_ref().map(|(a, _)| a.w as u32 != w || a.h as u32 != h).unwrap_or(true) {
            *canvases = Some((Canvas::new(w, h), Canvas::new(w, h)));
        }
        let (cs, cl) = canvases.as_mut().unwrap();
        let t = now();
        renderer.animating = false;
        renderer.wanted.clear();
        renderer.compose(cs, &st, OutputId::Screen, t, &self.images, &self.videos);
        renderer.compose(cl, &st, OutputId::Stream, t, &self.images, &self.videos);
        let on_air: Vec<String> = renderer.wanted.keys().cloned().collect();
        for (k, v) in self.preview.lock().unwrap().iter() {
            let e = renderer.wanted.entry(k.clone()).or_insert(false);
            *e |= *v;
        }
        self.videos.ensure_prioritized(&renderer.wanted, &on_air, (w, h));
        let mut f = self.frames.lock().unwrap();
        f.w = w;
        f.h = h;
        f.screen = Arc::new(cs.px.clone());
        f.stream = Arc::new(cl.px.clone());
        f.seq += 1;
        renderer.animating
    }

    fn run(self: Arc<Self>) {
        let mut renderer = Renderer::new();
        let mut canvases: Option<(Canvas, Canvas)> = None;
        let period = Duration::from_secs_f64(1.0 / FPS as f64);
        let mut next = Instant::now();
        let mut last_version = u64::MAX;
        let mut animating = true;
        let mut fps_count = 0u32;
        let mut fps_t = Instant::now();
        let mut idle_frames = 0u32;
        loop {
            let now_i = Instant::now();
            if now_i < next {
                std::thread::sleep(next - now_i);
            }
            next += period;
            if Instant::now() > next + period * 4 {
                next = Instant::now() + period;
            }
            let version = self.state.lock().unwrap().version;
            let previewing = !self.preview.lock().unwrap().is_empty();
            let need = animating || previewing || version != last_version || idle_frames > FPS;
            if need {
                let t0 = Instant::now();
                animating = self.render_once(&mut renderer, &mut canvases);
                last_version = version;
                idle_frames = 0;
                let ms = t0.elapsed().as_secs_f32() * 1000.0;
                let mut s = self.stats.lock().unwrap();
                s.render_ms = s.render_ms * 0.9 + ms * 0.1;
                if let Some(ctx) = self.ctx.lock().unwrap().as_ref() {
                    ctx.request_repaint();
                }
            } else {
                idle_frames += 1;
            }
            let frame = self.frames.lock().unwrap().stream.clone();
            {
                let mut sinks = self.sinks.lock().unwrap();
                sinks.retain(|s| s.is_active());
                for s in sinks.iter() {
                    s.push_video(&frame);
                }
            }
            fps_count += 1;
            if fps_t.elapsed() >= Duration::from_millis(500) {
                self.stats.lock().unwrap().fps = fps_count as f32 / fps_t.elapsed().as_secs_f32();
                fps_count = 0;
                fps_t = Instant::now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::locate_ffmpeg;
    use crate::model::{Destination, Quality, SendTarget, Slide, SlideKind, Transition};
    use crate::stream::{live_output_args, record_output_args, start_sink, StartSpec};

    #[test]
    #[ignore]
    fn records_and_multistreams_with_ffmpeg() {
        let ff = locate_ffmpeg("").expect("ffmpeg installed");
        let sh = Shared::new(LiveState::default(), Some(ff.path.clone()));
        sh.audio.start_mixer();
        sh.start();
        sh.edit(|s| s.send_slide(SendTarget::Both, Some(Slide::text(SlideKind::Scripture, "The Lord is my shepherd; I shall lack nothing.", "Psalm 23:1 WEB")), Transition::Cut));
        let dir = std::env::temp_dir().join("lampstand-selftest");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rec_path = dir.join("record.mp4");
        let d = |n: &str| Destination { service: "custom".into(), name: n.into(), server: dir.join(format!("{n}.flv")).to_string_lossy().to_string(), key: String::new(), enabled: true };
        let dests = vec![d("youtube"), d("facebook")];
        let canvas = sh.snapshot().canvas;
        let rec = start_sink(
            StartSpec { ffmpeg: &ff.path, canvas, quality: Quality::Hd720, kbps: 3000, encoder: "libx264", output: record_output_args(&rec_path), label: "rec".into(), dest_names: vec![], secrets: vec![], path: Some(rec_path.clone()), stats_period: ff.has_stats_period() },
            &sh.audio,
        )
        .unwrap();
        let live = start_sink(
            StartSpec { ffmpeg: &ff.path, canvas, quality: Quality::Hd720, kbps: 3000, encoder: "libx264", output: live_output_args(&dests), label: "live".into(), dest_names: vec![], secrets: vec![], path: None, stats_period: ff.has_stats_period() },
            &sh.audio,
        )
        .unwrap();
        sh.sinks.lock().unwrap().push(rec.clone());
        sh.sinks.lock().unwrap().push(live.clone());
        std::thread::sleep(Duration::from_secs(3));
        rec.set_paused(true);
        sh.edit(|s| s.brk.show.set(true));
        std::thread::sleep(Duration::from_secs(1));
        rec.set_paused(false);
        sh.edit(|s| s.brk.show.set(false));
        std::thread::sleep(Duration::from_secs(2));
        let st = live.stats.lock().unwrap().clone();
        println!("live state {:?} frames {} fps {} kbps {} log {:?}", st.state, st.frames, st.fps, st.kbps, st.log);
        rec.stop(&sh.audio);
        live.stop(&sh.audio);
        std::thread::sleep(Duration::from_secs(4));
        for f in [rec_path.clone(), dir.join("youtube.flv"), dir.join("facebook.flv")] {
            let out = std::process::Command::new(&ff.path).args(["-hide_banner", "-i"]).arg(&f).args(["-f", "null", "-"]).output().unwrap();
            let err = String::from_utf8_lossy(&out.stderr).to_string();
            let dur = err.lines().filter(|l| l.contains("Duration") || l.contains("Stream #")).collect::<Vec<_>>().join(" | ");
            println!("{}: {} bytes :: {}", f.display(), std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0), dur);
            assert!(std::fs::metadata(&f).map(|m| m.len()).unwrap_or(0) > 10_000, "{} too small", f.display());
            assert!(dur.contains("Video") && dur.contains("Audio"), "{} lacks audio or video", f.display());
        }
    }
}

#[cfg(test)]
mod media_tests {
    use super::*;
    use crate::audio::SrcKind;
    use crate::media::{locate_ffmpeg, video_key};
    use std::collections::HashMap;

    #[test]
    #[ignore]
    fn plays_video_and_music_files() {
        let ff = locate_ffmpeg("").expect("ffmpeg installed");
        let dir = std::env::temp_dir().join("lampstand-mediatest");
        let _ = std::fs::create_dir_all(&dir);
        let clip = dir.join("clip.mp4");
        let song = dir.join("song.m4a");
        let ok = std::process::Command::new(&ff.path)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=640x360:rate=30:duration=3", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-shortest", "-pix_fmt", "yuv420p"])
            .arg(&clip)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let ok = std::process::Command::new(&ff.path).args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=330:duration=2"]).arg(&song).status().unwrap().success();
        assert!(ok);
        let sh = Shared::new(LiveState::default(), Some(ff.path.clone()));
        sh.audio.start_mixer();
        let key = video_key(&clip.to_string_lossy(), true, true);
        let mut wanted = HashMap::new();
        wanted.insert(key.clone(), true);
        sh.videos.ensure_prioritized(&wanted, &[], (1280, 720));
        sh.audio.play("music", &song.to_string_lossy(), SrcKind::Music, false, 1.0, 0.0);
        let mut got_frame = false;
        let mut peak = 0.0f32;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(100));
            sh.videos.ensure_prioritized(&wanted, &[], (1280, 720));
            if let Some(f) = sh.videos.frame(&key) {
                got_frame = f.w == 1280 && f.h == 720 && f.data.iter().any(|&b| b > 50);
            }
            let m = *sh.audio.meters.lock().unwrap();
            peak = peak.max(m.music).max(m.media);
        }
        println!("frame {got_frame} peak {peak} errors {:?}", sh.videos.errors.lock().unwrap());
        sh.videos.stop_all();
        assert!(got_frame, "no video frame decoded");
        assert!(peak > 0.05, "no audio heard");
        let finished = sh.audio.take_finished();
        assert!(finished.iter().any(|(k, _)| k == "music"), "music did not report finishing: {finished:?}");
    }
}

#[cfg(test)]
mod screen_tests {
    use super::*;
    use crate::media::{camera_key, locate_ffmpeg};
    use std::collections::HashMap;

    #[test]
    #[ignore]
    fn screen_capture_reaches_the_picture() {
        let ff = locate_ffmpeg("").expect("ffmpeg installed");
        let sh = Shared::new(LiveState::default(), Some(ff.path.clone()));
        let key = camera_key("screen:2|Screen 1");
        let mut wanted = HashMap::new();
        wanted.insert(key.clone(), false);
        let mut got = false;
        for _ in 0..60 {
            sh.videos.ensure_prioritized(&wanted, &[], (1280, 720));
            if let Some(f) = sh.videos.frame(&key) {
                got = f.data.chunks(4).any(|p| p[0] > 20 || p[1] > 20 || p[2] > 20);
                if got {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        println!("errors {:?}", sh.videos.errors.lock().unwrap());
        sh.videos.stop_all();
        assert!(got, "no screen picture arrived");
    }

    #[test]
    #[ignore]
    fn renumbered_screen_and_window_still_open() {
        let ff = locate_ffmpeg("").expect("ffmpeg installed");
        let sh = Shared::new(LiveState::default(), Some(ff.path.clone()));
        for spec in ["screen:@0|Screen 1", "window:@0,100,100,800,600,1920,1080|Test window", "screen:9|Screen 2"] {
            let key = camera_key(spec);
            let mut wanted = HashMap::new();
            wanted.insert(key.clone(), false);
            let mut got = false;
            for _ in 0..60 {
                sh.videos.ensure_prioritized(&wanted, &[], (1280, 720));
                if sh.videos.frame(&key).is_some() {
                    got = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            println!("{spec}: picture {got}, waiting {:?}, errors {:?}", sh.videos.waiting_for(&key), sh.videos.error_for(&key));
            if !got {
                sh.videos.stop_all();
            }
            assert!(got, "{spec} gave no picture");
        }
        sh.videos.stop_all();
    }

    #[test]
    #[ignore]
    fn one_screen_at_a_time_by_priority() {
        let ff = locate_ffmpeg("").expect("ffmpeg installed");
        let sh = Shared::new(LiveState::default(), Some(ff.path.clone()));
        let tiles: Vec<String> = (0..3).map(|i| camera_key(&format!("screen:@{i}|Screen {}", i + 1))).collect();
        let mut wanted = HashMap::new();
        for k in &tiles {
            wanted.insert(k.clone(), false);
        }
        *sh.videos.focus.lock().unwrap() = Some(tiles[1].clone());
        let wait = |key: &str, on_air: &[String], wanted: &HashMap<String, bool>| {
            for _ in 0..60 {
                sh.videos.ensure_prioritized(wanted, on_air, (1280, 720));
                if sh.videos.frame(key).is_some() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            false
        };
        let a = wait(&tiles[1], &[], &wanted);
        println!("focused Screen 2 live: {a}, active {:?}", sh.videos.active_screen());
        let air = vec![tiles[2].clone()];
        let b = wait(&tiles[2], &air, &wanted);
        println!("on-air Screen 3 live: {b}, active {:?}, Screen 2 still live: {}", sh.videos.active_screen(), sh.videos.is_live_capture(&tiles[1]));
        sh.videos.stop_all();
        assert!(a && b);
    }
}
