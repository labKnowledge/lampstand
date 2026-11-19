use crate::canvas::{shade, vignette_mask, Canvas};
use crate::media::{camera_key, video_key, ImageStore, VideoHub};
use crate::model::*;
use crate::text::*;
use std::collections::HashMap;
use std::sync::Arc;

pub const GOLD: Rgb = [224, 178, 82];
pub const INK: Rgb = [16, 18, 28];
const TDUR: f64 = 0.6;
const ODUR: f64 = 0.55;

fn ease(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    1.0 - (1.0 - p).powi(3)
}

#[derive(Clone, Copy)]
struct Xf {
    dx: f32,
    dy: f32,
    alpha: f32,
    wipe: Option<(f32, f32)>,
}

impl Xf {
    fn id() -> Self {
        Xf { dx: 0.0, dy: 0.0, alpha: 1.0, wipe: None }
    }
    fn alpha(a: f32) -> Self {
        Xf { alpha: a, ..Xf::id() }
    }
}

fn pair(tr: Transition, p: f32, w: f32, h: f32, opaque_under: bool) -> (Xf, Xf) {
    let p = ease(p);
    match tr {
        Transition::Cut => (Xf::alpha(0.0), Xf::id()),
        Transition::Fade => (Xf::alpha(if opaque_under { 1.0 } else { 1.0 - p }), Xf::alpha(p)),
        Transition::Push => (Xf { dx: -p * w, ..Xf::id() }, Xf { dx: (1.0 - p) * w, ..Xf::id() }),
        Transition::Rise => (
            Xf { dy: -p * h * 0.03, alpha: if opaque_under { 1.0 } else { 1.0 - p }, ..Xf::id() },
            Xf { dy: (1.0 - p) * h * 0.06, alpha: p, ..Xf::id() },
        ),
        Transition::Wipe => (Xf { wipe: Some((p * w, w)), ..Xf::id() }, Xf { wipe: Some((0.0, p * w)), ..Xf::id() }),
    }
}

pub struct Prep {
    pub w: i32,
    pub h: i32,
    pub data: Vec<u8>,
    pub alpha: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Fit {
    Cover,
    Contain,
    Logo,
}

pub struct Renderer {
    pub text: TextEngine,
    w: u32,
    h: u32,
    vignette: Vec<u8>,
    bases: HashMap<(Rgb, Rgb), Arc<Vec<u8>>>,
    preps: HashMap<(String, Fit), Option<Arc<Prep>>>,
    pub wanted: HashMap<String, bool>,
    pub animating: bool,
    slide_floor: f32,
    box_top: f32,
    ticker_h: f32,
    ov_alpha: f32,
    gfx_alpha: f32,
    cam_adj: HashMap<String, CamAdjust>,
    area: (f32, f32, f32, f32),
    force_shadow: bool,
}

impl Renderer {
    pub fn new() -> Self {
        Renderer {
            text: TextEngine::new(),
            w: 0,
            h: 0,
            vignette: vec![],
            bases: HashMap::new(),
            preps: HashMap::new(),
            wanted: HashMap::new(),
            animating: false,
            slide_floor: 0.0,
            box_top: 0.0,
            ticker_h: 0.0,
            ov_alpha: 1.0,
            gfx_alpha: 1.0,
            cam_adj: HashMap::new(),
            area: (0.0, 0.0, 0.0, 0.0),
            force_shadow: false,
        }
    }

    fn ensure_size(&mut self, w: u32, h: u32) {
        if self.w != w || self.h != h {
            self.w = w;
            self.h = h;
            self.vignette = vignette_mask(w, h);
            self.bases.clear();
            self.preps.clear();
        }
    }

    fn base(&mut self, a: Rgb, b: Rgb) -> Arc<Vec<u8>> {
        let (w, h) = (self.w, self.h);
        self.bases
            .entry((a, b))
            .or_insert_with(|| {
                let mut c = Canvas::new(w, h);
                c.diagonal_gradient(a, b);
                Arc::new(c.px)
            })
            .clone()
    }

    fn prep(&mut self, images: &ImageStore, path: &str, fit: Fit) -> Option<Arc<Prep>> {
        let key = (path.to_string(), fit);
        if let Some(p) = self.preps.get(&key) {
            return p.clone();
        }
        let img = match images.get(path) {
            Some(Ok(img)) => img,
            Some(Err(())) => {
                self.preps.insert(key, None);
                return None;
            }
            None => {
                self.animating = true;
                return None;
            }
        };
        let (cw, ch) = (self.w as f32, self.h as f32);
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        let alpha = img.pixels().any(|p| p.0[3] < 250);
        let prep = match fit {
            Fit::Cover => {
                let s = (cw / iw).max(ch / ih);
                let (sw, sh) = ((iw * s).ceil() as u32, (ih * s).ceil() as u32);
                let scaled = image::imageops::resize(&*img, sw.max(1), sh.max(1), image::imageops::FilterType::Triangle);
                let ox = (sw.saturating_sub(self.w)) / 2;
                let oy = (sh.saturating_sub(self.h)) / 2;
                let crop = image::imageops::crop_imm(&scaled, ox, oy, self.w.min(sw), self.h.min(sh)).to_image();
                Prep { w: crop.width() as i32, h: crop.height() as i32, data: crop.into_raw(), alpha }
            }
            Fit::Contain | Fit::Logo => {
                let (mw, mh) = if fit == Fit::Logo { (cw * 0.2, ch * 0.12) } else { (cw, ch) };
                let s = (mw / iw).min(mh / ih);
                let (sw, sh) = ((iw * s).round().max(1.0) as u32, (ih * s).round().max(1.0) as u32);
                let scaled = image::imageops::resize(&*img, sw, sh, image::imageops::FilterType::Triangle);
                Prep { w: sw as i32, h: sh as i32, data: scaled.into_raw(), alpha }
            }
        };
        let p = Some(Arc::new(prep));
        self.preps.insert(key, p.clone());
        p
    }

    pub fn compose(&mut self, cv: &mut Canvas, st: &LiveState, out: OutputId, t: f64, images: &ImageStore, videos: &VideoHub) {
        self.ensure_size(cv.w as u32, cv.h as u32);
        let o = if st.mirror { &st.outputs[0] } else { st.out(out) };
        let look = st.look_for(out).clone();
        let (w, h) = (cv.w as f32, cv.h as f32);
        cv.reset_clip();
        cv.clear([0, 0, 0]);
        if self.cam_adj != st.cam_adjust {
            self.cam_adj = st.cam_adjust.clone();
        }
        let pc = ((t - o.clean_t0) / 0.5) as f32;
        if pc < 1.0 {
            self.animating = true;
        }
        let vis = if o.clean { 1.0 - ease(pc) } else { ease(pc) };
        let pip_a = ease(((t - o.pip_t0) / TDUR) as f32);
        if pip_a < 1.0 {
            self.animating = true;
        }
        let side = o.pip.is_some() && o.pip_layout == PipLayout::Side;
        let layout = look.layout;
        let ps = ((t - o.slide_t0) / TDUR) as f32;
        let in_trans = ps < 1.0 && o.transition != Transition::Cut;
        let has_text = |s: &Option<Slide>| s.as_ref().map(|s| s.kind != SlideKind::Media).unwrap_or(false);
        let now_text = if has_text(&o.slide) { 1.0 } else { 0.0 };
        let was_text = if has_text(&o.prev_slide) { 1.0 } else { 0.0 };
        let presence = if in_trans { was_text + (now_text - was_text) * ease(ps) } else { now_text };
        let tp = presence * vis;
        self.area = (0.0, 0.0, w, h);
        self.force_shadow = layout == TextLayout::Transparent;
        let side_text = layout.is_side() && !side && tp > 0.0;

        let pb = ((t - o.bg_t0) / TDUR) as f32;
        if side {
            let base = self.base([12, 14, 22], [40, 30, 52]);
            cv.blit(&base, cv.w, cv.h, 0, 0, 1.0, false);
            let tw = w * 0.475;
            let th = tw * 9.0 / 16.0;
            let ty = (h - th) / 2.0 - h * 0.04;
            let gap = w * 0.017;
            let lx = (w - tw * 2.0 - gap) / 2.0;
            cv.fill_rect(lx - 2.0, ty - 2.0, tw + 4.0, th + 4.0, [255, 255, 255], 0.18);
            self.source_in(cv, &o.background, lx, ty, tw, th, 1.0, t, &look, o.bg_t0, images, videos);
            if let Some(p) = &o.pip {
                let rx = lx + tw + gap;
                cv.fill_rect(rx - 2.0, ty - 2.0, tw + 4.0, th + 4.0, [255, 255, 255], 0.18 * pip_a);
                self.source_in(cv, p, rx, ty, tw, th, pip_a, t, &look, o.pip_t0, images, videos);
            }
        } else if side_text {
            if tp < 1.0 {
                self.animating = true;
            }
            let pw = w * 0.42;
            let base = self.base(WARM_BG.0, WARM_BG.1);
            cv.blit(&base, cv.w, cv.h, 0, 0, 1.0, false);
            cv.darken(&self.vignette, 0.5);
            let shown = pw * ease(tp);
            let (pic_x, panel_x) = if layout == TextLayout::SideLeft { (shown, shown - pw) } else { (0.0, w - shown) };
            self.source_in(cv, &o.background, pic_x, 0.0, w - shown, h, 1.0, t, &look, o.bg_t0, images, videos);
            let edge = if layout == TextLayout::SideLeft { pic_x } else { w - shown };
            cv.fill_rect(edge - 2.0, 0.0, 4.0, h, GOLD, ease(tp));
            self.area = (panel_x, 0.0, pw, h);
        } else if pb < 1.0 && o.prev_background.is_some() && o.transition != Transition::Cut {
            self.animating = true;
            let (xo, xn) = pair(o.transition, pb, w, h, true);
            if let Some(prev) = &o.prev_background {
                self.background(cv, prev, xo, t, &look, o.bg_t0 - 30.0, images, videos);
            }
            self.background(cv, &o.background, xn, t, &look, o.bg_t0, images, videos);
        } else {
            self.background(cv, &o.background, Xf::id(), t, &look, o.bg_t0, images, videos);
        }
        let k = h / 720.0;
        let ticker_on = st.ticker.place.on(out) && st.ticker.show.active(t, ODUR);
        self.ticker_h = if ticker_on { 44.0 * k * ease(st.ticker.show.amount(t, ODUR)) } else { 0.0 };
        let cap_fresh = st.caption.auto_clear <= 0.0 || ((t - st.caption.last_t) as f32) < st.caption.auto_clear + 0.4;
        let cap_h = if st.caption.place.on(out) && st.caption.show.visible && cap_fresh && !st.caption.lines.is_empty() {
            self.caption_layout(&st.caption, w, k).map(|l| l.height() + 22.0 * k + 12.0 * k).unwrap_or(0.0)
        } else {
            0.0
        };
        self.slide_floor = h - 26.0 * k - self.ticker_h - cap_h;
        self.box_top = h;
        match layout {
            TextLayout::Full if !side => {
                if look.dim > 0.0 && tp > 0.0 {
                    cv.fill_rect(0.0, 0.0, w, h, [0, 0, 0], look.dim * tp);
                }
                if look.vignette {
                    cv.darken(&self.vignette, 0.65);
                }
            }
            TextLayout::Cover if !side => {
                if tp > 0.0 {
                    let base = self.base(WARM_BG.0, WARM_BG.1);
                    cv.blit(&base, cv.w, cv.h, 0, 0, tp, false);
                }
                if look.vignette {
                    cv.darken(&self.vignette, 0.65 * tp.max(0.0));
                }
            }
            TextLayout::LowerThird if look.vignette && !side => cv.darken(&self.vignette, 0.4),
            _ => {}
        }
        if let (Some(p), PipLayout::Corner) = (&o.pip, o.pip_layout) {
            let pw = w * o.pip_size.clamp(0.2, 0.55);
            let ph = pw * 9.0 / 16.0;
            let m = w * 0.03;
            let px = if o.pip_corner % 2 == 0 { m } else { w - pw - m };
            let py = if o.pip_corner < 2 { h * 0.12 } else { h - ph - h * 0.2 };
            let k2 = h / 720.0;
            let bw = (4.0 * k2).max(2.0);
            cv.round_rect(px - bw + 8.0 * k2, py - bw + 10.0 * k2, pw + bw * 2.0, ph + bw * 2.0, 10.0 * k2, [0, 0, 0], 0.4 * pip_a);
            cv.round_rect(px - bw, py - bw, pw + bw * 2.0, ph + bw * 2.0, 8.0 * k2, [255, 255, 255], 0.95 * pip_a);
            self.source_in(cv, p, px, py, pw, ph, pip_a, t, &look, o.pip_t0, images, videos);
            if matches!(layout, TextLayout::Full | TextLayout::Cover | TextLayout::Transparent) {
                let free_x = if o.pip_corner % 2 == 0 { px + pw + m * 0.5 } else { 0.0 };
                let free_w = w - pw - m * 1.5;
                let shrink = pip_a;
                self.area = (free_x * shrink, 0.0, w - (w - free_w) * shrink, h);
            }
        }


        if vis <= 0.0 {
        } else if in_trans {
            self.animating = true;
            let (mut xo, mut xn) = pair(o.transition, ps, w, h, false);
            xo.alpha *= vis;
            xn.alpha *= vis;
            if let Some(prev) = &o.prev_slide {
                self.slide(cv, prev, &look, layout, xo, st, t, images, videos);
            }
            if let Some(cur) = &o.slide {
                self.slide(cv, cur, &look, layout, xn, st, t, images, videos);
            }
        } else if let Some(cur) = &o.slide {
            self.slide(cv, cur, &look, layout, Xf::alpha(vis), st, t, images, videos);
        }

        let brk_on = st.brk.place.on(out);
        let brk_a = if brk_on { ease(st.brk.show.amount(t, 0.8)) } else { 0.0 };
        if brk_on && st.brk.show.active(t, 0.8) {
            self.animating = true;
            if brk_a > 0.0 {
                self.break_scene(cv, st, t, brk_a, images, videos);
            }
        }

        if brk_a > 0.0 {
            self.box_top = h;
        }
        self.ov_alpha = (1.0 - brk_a) * vis;
        self.gfx_alpha = vis;
        if st.title.place.on(out) && st.title.show.active(t, ODUR) && self.ov_alpha > 0.0 {
            self.title_card(cv, &st.title, t, 1.0);
        }
        if st.lower_third.place.on(out) && st.lower_third.show.active(t, ODUR) && self.ov_alpha > 0.0 {
            let floor = (h - 68.0 * k - self.ticker_h - cap_h).min(self.box_top - 14.0 * k);
            self.lower_third(cv, &st.lower_third, t, floor);
        }
        if ticker_on {
            self.ticker(cv, &st.ticker, t);
        }
        if st.caption.place.on(out) && st.caption.show.active(t, ODUR) && self.ov_alpha > 0.0 {
            self.caption(cv, &st.caption, t);
        }
        self.ov_alpha = 1.0;
        if st.logo.place.on(out) && st.logo.show.active(t, ODUR) && vis > 0.0 {
            self.logo(cv, &st.logo, t, out == OutputId::Stream && st.is_live, images);
        }
        self.gfx_alpha = 1.0;

        let pblack = ((t - o.black_t0) / 0.5) as f32;
        let black = if o.black { ease(pblack) } else { 1.0 - ease(pblack) };
        if pblack < 1.0 {
            self.animating = true;
        }
        if black > 0.0 {
            cv.reset_clip();
            cv.fill_rect(0.0, 0.0, w, h, [0, 0, 0], black);
        }
        cv.reset_clip();
    }

    fn apply_wipe(&self, cv: &mut Canvas, xf: Xf) {
        if let Some((a, b)) = xf.wipe {
            cv.set_clip(a, 0.0, b, cv.h as f32);
        } else {
            cv.reset_clip();
        }
    }

    fn background(&mut self, cv: &mut Canvas, bg: &Background, xf: Xf, t: f64, look: &Look, t0: f64, images: &ImageStore, videos: &VideoHub) {
        if xf.alpha <= 0.0 {
            return;
        }
        self.apply_wipe(cv, xf);
        let (w, h) = (cv.w as f32, cv.h as f32);
        match bg {
            Background::Color(c) => cv.fill_rect(xf.dx, xf.dy, w, h, *c, xf.alpha),
            Background::Gradient(a, b) => {
                let base = self.base(*a, *b);
                cv.blit(&base, cv.w, cv.h, xf.dx.round() as i32, xf.dy.round() as i32, xf.alpha, false);
            }
            Background::Image(p) => {
                if let Some(img) = self.prep(images, p, Fit::Cover) {
                    if look.slow_zoom {
                        self.animating = true;
                        let z = 1.0 + 0.06 * (0.5 - 0.5 * (((t - t0) / 24.0) * std::f64::consts::TAU).cos()) as f32;
                        let (zw, zh) = (w * z, h * z);
                        let drift = ((t - t0) / 31.0).sin() as f32 * (zw - w) * 0.3;
                        cv.blit_scaled(&img.data, img.w, img.h, xf.dx - (zw - w) / 2.0 + drift, xf.dy - (zh - h) / 2.0, zw, zh, xf.alpha, false);
                    } else {
                        cv.blit(&img.data, img.w, img.h, xf.dx.round() as i32, xf.dy.round() as i32, xf.alpha, false);
                    }
                }
            }
            Background::Video(p) => {
                let key = video_key(p, true, false);
                self.video(cv, &key, xf, videos);
            }
            Background::Camera(c) => {
                let key = camera_key(c);
                let adj = self.cam_adj.get(c).copied().unwrap_or_default();
                self.video_rect(cv, &key, xf.dx, xf.dy, w, h, xf.alpha, adj, videos);
            }
        }
        cv.reset_clip();
    }

    fn source_in(&mut self, cv: &mut Canvas, bg: &Background, x: f32, y: f32, w: f32, h: f32, alpha: f32, t: f64, look: &Look, t0: f64, images: &ImageStore, videos: &VideoHub) {
        if alpha <= 0.0 {
            return;
        }
        let saved = cv.clip;
        let nx0 = (x.round() as i32).max(saved.x0);
        let ny0 = (y.round() as i32).max(saved.y0);
        let nx1 = ((x + w).round() as i32).min(saved.x1);
        let ny1 = ((y + h).round() as i32).min(saved.y1);
        if nx1 <= nx0 || ny1 <= ny0 {
            return;
        }
        cv.clip = crate::canvas::Clip { x0: nx0, y0: ny0, x1: nx1, y1: ny1 };
        let (rx, ry, rw, rh) = (x, y, w, h);
        let (w, h) = if rw / rh > 16.0 / 9.0 { (rw, rw * 9.0 / 16.0) } else { (rh * 16.0 / 9.0, rh) };
        let (x, y) = (rx + (rw - w) / 2.0, ry + (rh - h) / 2.0);
        match bg {
            Background::Color(c) => cv.fill_rect(rx, ry, rw, rh, *c, alpha),
            Background::Gradient(a, b) => {
                let base = self.base(*a, *b);
                cv.blit_scaled(&base, cv.w, cv.h, x, y, w, h, alpha, false);
            }
            Background::Image(p) => {
                if let Some(img) = self.prep(images, p, Fit::Cover) {
                    cv.blit_scaled(&img.data, img.w, img.h, x, y, w, h, alpha, false);
                }
            }
            Background::Video(p) => {
                let key = video_key(p, true, false);
                self.video_rect(cv, &key, x, y, w, h, alpha, CamAdjust::default(), videos);
            }
            Background::Camera(c) => {
                let key = camera_key(c);
                let adj = self.cam_adj.get(c).copied().unwrap_or_default();
                self.video_rect(cv, &key, x, y, w, h, alpha, adj, videos);
            }
        }
        let _ = (t, look, t0);
        cv.clip = saved;
    }

    fn video_rect(&mut self, cv: &mut Canvas, key: &str, x: f32, y: f32, w: f32, h: f32, alpha: f32, adj: CamAdjust, videos: &VideoHub) {
        let e = self.wanted.entry(key.to_string()).or_insert(false);
        *e |= crate::media::key_has_sound(key);
        self.animating = true;
        let Some(f) = videos.frame(key) else { return };
        let z = adj.zoom.clamp(1.0, 4.0);
        let (zw, zh) = (w * z, h * z);
        let ox = x - (zw - w) / 2.0 - adj.x.clamp(-1.0, 1.0) * (zw - w) / 2.0;
        let oy = y - (zh - h) / 2.0 - adj.y.clamp(-1.0, 1.0) * (zh - h) / 2.0;
        let saved = cv.clip;
        let nx0 = (x.round() as i32).max(saved.x0);
        let ny0 = (y.round() as i32).max(saved.y0);
        let nx1 = ((x + w).round() as i32).min(saved.x1);
        let ny1 = ((y + h).round() as i32).min(saved.y1);
        if nx1 > nx0 && ny1 > ny0 {
            cv.clip = crate::canvas::Clip { x0: nx0, y0: ny0, x1: nx1, y1: ny1 };
            if z == 1.0 && f.w as f32 == w && f.h as f32 == h {
                cv.blit(&f.data, f.w as i32, f.h as i32, x.round() as i32, y.round() as i32, alpha, false);
            } else {
                cv.blit_scaled(&f.data, f.w as i32, f.h as i32, ox, oy, zw, zh, alpha, false);
            }
            cv.clip = saved;
        }
    }

    fn video(&mut self, cv: &mut Canvas, key: &str, xf: Xf, videos: &VideoHub) {
        let sound = crate::media::key_has_sound(key);
        let e = self.wanted.entry(key.to_string()).or_insert(false);
        *e |= sound;
        self.animating = true;
        if let Some(f) = videos.frame(key) {
            if f.w as i32 == cv.w && f.h as i32 == cv.h {
                cv.blit(&f.data, f.w as i32, f.h as i32, xf.dx.round() as i32, xf.dy.round() as i32, xf.alpha, false);
            } else {
                cv.blit_scaled(&f.data, f.w as i32, f.h as i32, xf.dx, xf.dy, cv.w as f32, cv.h as f32, xf.alpha, false);
            }
        }
    }

    fn slide(&mut self, cv: &mut Canvas, s: &Slide, look: &Look, layout: TextLayout, xf: Xf, _st: &LiveState, t: f64, images: &ImageStore, videos: &VideoHub) {
        if xf.alpha <= 0.0 {
            return;
        }
        let (w, h) = (cv.w as f32, cv.h as f32);
        self.apply_wipe(cv, xf);
        match s.kind {
            SlideKind::Media => {
                cv.fill_rect(xf.dx, xf.dy, w, h, [0, 0, 0], xf.alpha);
                match &s.media {
                    Some(MediaRef::Image(p)) => {
                        if let Some(img) = self.prep(images, p, Fit::Contain) {
                            let x = xf.dx + (w - img.w as f32) / 2.0;
                            let y = xf.dy + (h - img.h as f32) / 2.0;
                            cv.blit(&img.data, img.w, img.h, x.round() as i32, y.round() as i32, xf.alpha, img.alpha);
                        }
                    }
                    Some(MediaRef::Video { path, sound, looping }) => {
                        let key = video_key(path, *looping, *sound);
                        self.video(cv, &key, xf, videos);
                    }
                    None => {}
                }
            }
            _ => {
                if layout == TextLayout::LowerThird {
                    self.slide_lower(cv, s, look, xf);
                } else {
                    if layout.is_side() {
                        cv.reset_clip();
                    }
                    self.slide_full(cv, s, look, xf);
                }
            }
        }
        cv.reset_clip();
        let _ = t;
    }

    fn slide_full(&mut self, cv: &mut Canvas, s: &Slide, look: &Look, xf: Xf) {
        let (ax, ay, w, h) = self.area;
        let k = cv.h as f32 / 720.0;
        let z = look.size.clamp(0.5, 1.8);
        let xf = Xf { dx: xf.dx + ax, dy: xf.dy + ay, ..xf };
        let mx = w * 0.08;
        let area_w = w - mx * 2.0;
        let main_font = font_index(look.font);
        let paint = Paint { color: look.color, alpha: xf.alpha, shadow: look.shadow || self.force_shadow, outline: look.outline };
        let text = if s.kind == SlideKind::Lyrics && look.uppercase_lyrics { s.text.to_uppercase() } else { s.text.clone() };
        match s.kind {
            SlideKind::Scripture => {
                let ref_px = 30.0 * k * z.min(1.3);
                let gap = 22.0 * k;
                let max_h = h * 0.74 - ref_px - gap;
                let laid = self.text.fit(main_font, &text, area_w, max_h, 64.0 * k * z, 22.0 * k);
                let total = laid.height() + gap + ref_px * 1.2;
                let y0 = (h - total) / 2.0 + xf.dy - h * 0.02;
                if look.boxed {
                    let bw = laid.width().max(w * 0.4) + 80.0 * k;
                    cv.round_rect((w - bw) / 2.0 + xf.dx, y0 - 34.0 * k, bw, total + 68.0 * k, 18.0 * k, [0, 0, 0], 0.5 * xf.alpha);
                }
                self.text.draw(cv, &laid, mx + xf.dx, y0, area_w, Align::Center, paint);
                let ry = y0 + laid.height() + gap;
                let rw = self.text.measure(F_SEMI, ref_px, &s.caption);
                cv.fill_rect(w / 2.0 - 24.0 * k + xf.dx, ry - 8.0 * k, 48.0 * k, 2.0 * k.max(1.0), GOLD, xf.alpha * 0.9);
                self.text.line(cv, F_SEMI, ref_px, &s.caption, w / 2.0 + xf.dx, ry, Align::Center, Paint { color: GOLD, ..paint });
                let _ = rw;
            }
            SlideKind::Lyrics => {
                let laid = self.text.fit(main_font, &text, area_w, h * 0.72, 66.0 * k * z, 24.0 * k);
                let y0 = (h - laid.height()) / 2.0 + xf.dy;
                if look.boxed {
                    let bw = laid.width() + 80.0 * k;
                    cv.round_rect((w - bw) / 2.0 + xf.dx, y0 - 30.0 * k, bw, laid.height() + 60.0 * k, 18.0 * k, [0, 0, 0], 0.5 * xf.alpha);
                }
                self.text.draw(cv, &laid, mx + xf.dx, y0, area_w, Align::Center, paint);
                if !s.caption.is_empty() {
                    self.text.line(cv, F_SANS, 18.0 * k, &s.caption, w / 2.0 + xf.dx, h - 44.0 * k + xf.dy, Align::Center, Paint::plain(look.color, xf.alpha * 0.55));
                }
            }
            SlideKind::Announcement | SlideKind::Title => {
                let mut lines = text.lines();
                let head = lines.next().unwrap_or("").to_string();
                let body = lines.collect::<Vec<_>>().join("\n");
                let (hp, bp, hf) = if s.kind == SlideKind::Title { (96.0, 40.0, F_SERIF) } else { (74.0, 38.0, F_SERIF) };
                let hl = self.text.fit(hf, &head, area_w, h * 0.4, hp * k * z, 28.0 * k);
                let bl = self.text.fit(F_SANS, &body, area_w * 0.9, h * 0.35, bp * k * z, 18.0 * k);
                let gap = if body.is_empty() { 0.0 } else { 28.0 * k };
                let total = hl.height() + gap + bl.height();
                let y0 = (h - total) / 2.0 + xf.dy;
                if look.boxed {
                    let bw = hl.width().max(bl.width()) + 90.0 * k;
                    cv.round_rect((w - bw) / 2.0 + xf.dx, y0 - 34.0 * k, bw, total + 68.0 * k, 18.0 * k, [0, 0, 0], 0.5 * xf.alpha);
                }
                self.text.draw(cv, &hl, mx + xf.dx, y0, area_w, Align::Center, paint);
                if !body.is_empty() {
                    let by = y0 + hl.height() + gap;
                    cv.fill_rect(w / 2.0 - 30.0 * k + xf.dx, by - gap * 0.55, 60.0 * k, 3.0 * k.max(1.0), GOLD, xf.alpha);
                    let c = if s.kind == SlideKind::Title { GOLD } else { look.color };
                    self.text.draw(cv, &bl, mx + xf.dx, by, area_w, Align::Center, Paint { color: c, ..paint });
                }
            }
            SlideKind::Media => {}
        }
    }

    fn slide_lower(&mut self, cv: &mut Canvas, s: &Slide, look: &Look, xf: Xf) {
        let (w, h) = (cv.w as f32, cv.h as f32);
        let k = h / 720.0;
        let z = look.size.clamp(0.5, 1.5);
        let bx = w * 0.06 + xf.dx;
        let bw = w * 0.88;
        let pad = 26.0 * k;
        let bottom = self.slide_floor + xf.dy;
        let main_font = font_index(look.font);
        let text = if s.kind == SlideKind::Lyrics && look.uppercase_lyrics { s.text.to_uppercase() } else { s.text.clone() };
        let (laid, align) = match s.kind {
            SlideKind::Lyrics => (self.text.fit(main_font, &text, bw - pad * 2.0, h * 0.2, 36.0 * k * z, 18.0 * k), Align::Center),
            SlideKind::Scripture => (self.text.fit(main_font, &text, bw - pad * 2.0, h * 0.2, 32.0 * k * z, 16.0 * k), Align::Left),
            _ => {
                let one = text.lines().collect::<Vec<_>>().join("  ·  ");
                (self.text.fit(F_SEMI, &one, bw - pad * 2.0, h * 0.16, 36.0 * k * z, 18.0 * k), Align::Left)
            }
        };
        let bh = laid.height() + pad * 2.0;
        let by = bottom - bh;
        let chip_extra = if s.kind == SlideKind::Scripture && !s.caption.is_empty() { 22.0 * k } else { 0.0 };
        if xf.alpha > 0.3 {
            self.box_top = self.box_top.min(by - chip_extra);
        }
        cv.round_rect(bx, by, bw, bh, 10.0 * k, INK, 0.84 * xf.alpha);
        cv.fill_rect(bx, by + 10.0 * k, 5.0 * k, bh - 20.0 * k, GOLD, xf.alpha);
        let paint = Paint { color: look.color, alpha: xf.alpha, shadow: false, outline: look.outline };
        self.text.draw(cv, &laid, bx + pad, by + pad, bw - pad * 2.0, align, paint);
        if s.kind == SlideKind::Scripture && !s.caption.is_empty() {
            let px = 20.0 * k;
            let tw = self.text.measure(F_BOLD, px, &s.caption) + 28.0 * k;
            let ch = 34.0 * k;
            cv.round_rect(bx + pad, by - ch * 0.6, tw, ch, 8.0 * k, GOLD, xf.alpha);
            self.text.line(cv, F_BOLD, px, &s.caption, bx + pad + 14.0 * k, by - ch * 0.6 + 5.0 * k, Align::Left, Paint::plain(INK, xf.alpha));
        }
    }

    fn break_scene(&mut self, cv: &mut Canvas, st: &LiveState, t: f64, a: f32, images: &ImageStore, videos: &VideoHub) {
        let (w, h) = (cv.w as f32, cv.h as f32);
        let k = h / 720.0;
        let b = &st.brk;
        cv.reset_clip();
        let el = (t - b.show.t0).max(0.0);
        if b.ads.is_empty() {
            let base = self.base(WARM_BG.0, WARM_BG.1);
            cv.blit(&base, cv.w, cv.h, 0, 0, a, false);
            cv.darken(&self.vignette, 0.6 * a);
            self.text.line(cv, F_SERIF, 84.0 * k, &b.message, w / 2.0, h * 0.30, Align::Center, Paint::shadowed([255, 255, 255], a));
            let sub = self.text.fit(F_SANS, &b.sub, w * 0.7, h * 0.15, 32.0 * k, 16.0 * k);
            self.text.draw(cv, &sub, w * 0.15, h * 0.30 + 120.0 * k, w * 0.7, Align::Center, Paint::plain([230, 225, 240], a * 0.9));
            if let Some(end) = b.ends_at {
                let txt = countdown_text(end - t);
                self.text.line(cv, F_BOLD, 96.0 * k, &txt, w / 2.0, h * 0.58, Align::Center, Paint::plain(GOLD, a));
            }
            return;
        }
        let n = b.ads.len();
        let period = (b.ad_seconds as f64).max(2.0);
        let idx = (el / period) as usize % n;
        let local = (el % period) as f32;
        if local < 0.7 && el > period {
            let prev = (idx + n - 1) % n;
            self.ad(cv, &b.ads[prev], a, images, videos);
            self.ad(cv, &b.ads[idx], a * ease(local / 0.7), images, videos);
        } else {
            self.ad(cv, &b.ads[idx], a, images, videos);
        }
        let bh = 118.0 * k;
        let by = h - bh - self.ticker_h;
        cv.vgradient(0.0, by - 60.0 * k, w, 60.0 * k, [0, 0, 0], [0, 0, 0], 0.0, 0.55 * a);
        cv.fill_rect(0.0, by, w, bh, INK, 0.88 * a);
        cv.fill_rect(0.0, by, w, 4.0 * k, GOLD, a);
        self.text.line(cv, F_SERIF, 42.0 * k, &b.message, w * 0.05, by + 16.0 * k, Align::Left, Paint::plain([255, 255, 255], a));
        self.text.line(cv, F_SANS, 22.0 * k, &b.sub, w * 0.05, by + 72.0 * k, Align::Left, Paint::plain([200, 200, 214], a));
        if let Some(end) = b.ends_at {
            let txt = countdown_text(end - t);
            let px = 36.0 * k;
            let tw = self.text.measure(F_BOLD, px, &txt) + 44.0 * k;
            let label_px = 16.0 * k;
            let x = w * 0.95 - tw;
            cv.round_rect(x, by + 30.0 * k, tw, 62.0 * k, 31.0 * k, GOLD, a);
            self.text.line(cv, F_SEMI, label_px, "BACK IN", x + tw / 2.0, by + 10.0 * k, Align::Center, Paint::plain([200, 200, 214], a));
            self.text.line(cv, F_BOLD, px, &txt, x + tw / 2.0, by + 38.0 * k, Align::Center, Paint::plain(INK, a));
        }
        let dots_y = by - 22.0 * k;
        if n > 1 {
            for i in 0..n {
                let x = w / 2.0 + (i as f32 - (n as f32 - 1.0) / 2.0) * 18.0 * k;
                let c = if i == idx { GOLD } else { [255, 255, 255] };
                cv.round_rect(x - 4.0 * k, dots_y, 8.0 * k, 8.0 * k, 4.0 * k, c, a * if i == idx { 1.0 } else { 0.5 });
            }
        }
    }

    fn ad(&mut self, cv: &mut Canvas, ad: &AdItem, a: f32, images: &ImageStore, videos: &VideoHub) {
        if a <= 0.0 {
            return;
        }
        let (w, h) = (cv.w as f32, cv.h as f32);
        let k = h / 720.0;
        match ad {
            AdItem::Text { title, body, color } => {
                let base = self.base(*color, shade(*color, 0.35));
                cv.blit(&base, cv.w, cv.h, 0, 0, a, false);
                cv.darken(&self.vignette, 0.5 * a);
                let hl = self.text.fit(F_SERIF, title, w * 0.8, h * 0.3, 82.0 * k, 30.0 * k);
                let bl = self.text.fit(F_SANS, body, w * 0.7, h * 0.2, 38.0 * k, 18.0 * k);
                let total = hl.height() + 30.0 * k + bl.height();
                let y0 = (h - 118.0 * k - total) / 2.0;
                self.text.draw(cv, &hl, w * 0.1, y0, w * 0.8, Align::Center, Paint::shadowed([255, 255, 255], a));
                cv.fill_rect(w / 2.0 - 30.0 * k, y0 + hl.height() + 10.0 * k, 60.0 * k, 3.0 * k, GOLD, a);
                self.text.draw(cv, &bl, w * 0.15, y0 + hl.height() + 30.0 * k, w * 0.7, Align::Center, Paint::plain([235, 235, 245], a));
            }
            AdItem::Image(p) => {
                cv.fill_rect(0.0, 0.0, w, h, [0, 0, 0], a);
                if let Some(img) = self.prep(images, p, Fit::Cover) {
                    cv.blit(&img.data, img.w, img.h, 0, 0, a, false);
                }
            }
            AdItem::Video(p) => {
                cv.fill_rect(0.0, 0.0, w, h, [0, 0, 0], a);
                let key = video_key(p, true, true);
                self.video(cv, &key, Xf::alpha(a), videos);
            }
        }
    }

    fn anim_xf(&self, anim: Anim, p: f32, x0: f32, width: f32, k: f32) -> Xf {
        let e = ease(p);
        match anim {
            Anim::Slide => Xf { dx: -(1.0 - e) * (width + x0 + 40.0), ..Xf::id() },
            Anim::Fade => Xf::alpha(e),
            Anim::Wipe => Xf { wipe: Some((x0 - 10.0, x0 + e * (width + 20.0))), ..Xf::id() },
            Anim::Rise => Xf { dy: (1.0 - e) * 40.0 * k, alpha: e, ..Xf::id() },
        }
    }

    fn lower_third(&mut self, cv: &mut Canvas, lt: &LowerThird, t: f64, floor: f32) {
        let p = lt.show.amount(t, ODUR);
        if p < 1.0 && lt.show.active(t, ODUR) {
            self.animating = true;
        }
        if p <= 0.0 {
            return;
        }
        let (w, h) = (cv.w as f32, cv.h as f32);
        let k = h / 720.0;
        let name_px = 34.0 * k;
        let title_px = 22.0 * k;
        let nw = self.text.measure(F_BOLD, name_px, &lt.name);
        let tw = self.text.measure(F_SANS, title_px, &lt.title);
        let x0 = w * 0.06;
        let pad = 22.0 * k;
        let bw = (nw.max(tw) + pad * 2.0 + 10.0 * k).max(w * 0.24);
        let bh = 92.0 * k;
        let by = floor - bh;
        let xf = self.anim_xf(lt.anim, p, x0, bw, k);
        let a = xf.alpha * self.ov_alpha;
        let bx = x0 + xf.dx;
        let by = by + xf.dy;
        if let Some((c0, c1)) = xf.wipe {
            cv.set_clip(c0, 0.0, c1, h);
        }
        match lt.style {
            LtStyle::Classic => {
                cv.fill_rect(bx, by, 10.0 * k, bh, lt.accent, a);
                cv.fill_rect(bx + 10.0 * k, by, bw, bh, INK, 0.9 * a);
                self.text.line(cv, F_BOLD, name_px, &lt.name, bx + 10.0 * k + pad, by + 12.0 * k, Align::Left, Paint::plain([255, 255, 255], a));
                self.text.line(cv, F_SANS, title_px, &lt.title, bx + 10.0 * k + pad, by + 56.0 * k, Align::Left, Paint::plain(lt.accent, a));
            }
            LtStyle::Clean => {
                cv.round_rect(bx, by, bw, bh, 14.0 * k, [250, 249, 246], 0.97 * a);
                cv.fill_rect(bx + pad, by + bh - 14.0 * k, 46.0 * k, 4.0 * k, lt.accent, a);
                self.text.line(cv, F_BOLD, name_px, &lt.name, bx + pad, by + 10.0 * k, Align::Left, Paint::plain(INK, a));
                self.text.line(cv, F_SEMI, title_px, &lt.title, bx + pad, by + 50.0 * k, Align::Left, Paint::plain(shade(lt.accent, 0.75), a));
            }
            LtStyle::Banner => {
                cv.hgradient(bx - x0, by, w * 0.75, bh, shade(lt.accent, 0.55), INK, 0.95 * a, 0.0);
                self.text.line(cv, F_SERIF, name_px * 1.1, &lt.name, bx, by + 8.0 * k, Align::Left, Paint::shadowed([255, 255, 255], a));
                self.text.line(cv, F_SEMI, title_px, &lt.title.to_uppercase(), bx, by + 58.0 * k, Align::Left, Paint::plain([240, 230, 210], a));
            }
            LtStyle::Minimal => {
                self.text.line(cv, F_BOLD, name_px, &lt.name, bx, by + 6.0 * k, Align::Left, Paint::shadowed([255, 255, 255], a));
                cv.fill_rect(bx, by + 52.0 * k, (nw * 0.6).max(60.0 * k), 3.0 * k, lt.accent, a);
                self.text.line(cv, F_SANS, title_px, &lt.title, bx, by + 60.0 * k, Align::Left, Paint::shadowed([235, 235, 235], a));
            }
        }
        cv.reset_clip();
    }

    fn title_card(&mut self, cv: &mut Canvas, tc: &TitleCard, t: f64, _s: f32) {
        let p = tc.show.amount(t, ODUR);
        if p < 1.0 && tc.show.active(t, ODUR) {
            self.animating = true;
        }
        if p <= 0.0 {
            return;
        }
        let (w, h) = (cv.w as f32, cv.h as f32);
        let k = h / 720.0;
        if tc.centered {
            let xf = self.anim_xf(tc.anim, p, 0.0, w, k);
            let a = xf.alpha * self.ov_alpha;
            if let Some((c0, c1)) = xf.wipe {
                cv.set_clip(c0, 0.0, c1, h);
            }
            let by = h * 0.36 + xf.dy;
            cv.vgradient(xf.dx, by, w, 70.0 * k, [0, 0, 0], [0, 0, 0], 0.0, 0.6 * a);
            cv.fill_rect(xf.dx, by + 70.0 * k, w, 110.0 * k, [0, 0, 0], 0.6 * a);
            cv.vgradient(xf.dx, by + 180.0 * k, w, 70.0 * k, [0, 0, 0], [0, 0, 0], 0.6 * a, 0.0);
            self.text.line(cv, F_SERIF, 64.0 * k, &tc.text, w / 2.0 + xf.dx, by + 62.0 * k, Align::Center, Paint::shadowed([255, 255, 255], a));
            if !tc.sub.is_empty() {
                self.text.line(cv, F_SEMI, 24.0 * k, &tc.sub.to_uppercase(), w / 2.0 + xf.dx, by + 152.0 * k, Align::Center, Paint::plain(GOLD, a));
            }
        } else {
            let x0 = w * 0.045;
            let tp = 32.0 * k;
            let sp = 18.0 * k;
            let tw = self.text.measure(F_SERIF, tp, &tc.text);
            let sw = self.text.measure(F_SEMI, sp, &tc.sub.to_uppercase());
            let bw = tw.max(sw) + 48.0 * k;
            let bh = if tc.sub.is_empty() { 62.0 * k } else { 92.0 * k };
            let xf = self.anim_xf(tc.anim, p, x0, bw, k);
            let a = xf.alpha * self.ov_alpha;
            if let Some((c0, c1)) = xf.wipe {
                cv.set_clip(c0, 0.0, c1, h);
            }
            let bx = x0 + xf.dx;
            let by = h * 0.06 + xf.dy;
            cv.round_rect(bx, by, bw, bh, 10.0 * k, INK, 0.86 * a);
            cv.fill_rect(bx, by + 12.0 * k, 4.0 * k, bh - 24.0 * k, GOLD, a);
            if !tc.sub.is_empty() {
                self.text.line(cv, F_SEMI, sp, &tc.sub.to_uppercase(), bx + 24.0 * k, by + 12.0 * k, Align::Left, Paint::plain(GOLD, a));
                self.text.line(cv, F_SERIF, tp, &tc.text, bx + 24.0 * k, by + 36.0 * k, Align::Left, Paint::plain([255, 255, 255], a));
            } else {
                self.text.line(cv, F_SERIF, tp, &tc.text, bx + 24.0 * k, by + 10.0 * k, Align::Left, Paint::plain([255, 255, 255], a));
            }
        }
        cv.reset_clip();
    }

    fn ticker(&mut self, cv: &mut Canvas, tk: &Ticker, t: f64) {
        self.animating = true;
        let p = ease(tk.show.amount(t, ODUR));
        if p <= 0.0 {
            return;
        }
        let (w, h) = (cv.w as f32, cv.h as f32);
        let k = h / 720.0;
        let bh = 44.0 * k;
        let by = h - bh * p;
        let ga = self.gfx_alpha;
        cv.fill_rect(0.0, by, w, bh, INK, 0.92 * ga);
        let label = "UPDATES";
        let lp = 17.0 * k;
        let lw = self.text.measure(F_BOLD, lp, label) + 36.0 * k;
        let px = 21.0 * k;
        let text = format!("{}        ", tk.text);
        let tw = self.text.measure(F_SEMI, px, &text).max(1.0);
        let speed = tk.speed.max(10.0) * k;
        let off = ((t - tk.show.t0) as f32 * speed) % tw;
        cv.set_clip(lw, by, w, by + bh);
        let mut x = lw + 20.0 * k - off;
        while x < w {
            self.text.line(cv, F_SEMI, px, &text, x, by + 10.0 * k, Align::Left, Paint::plain([240, 240, 248], ga));
            x += tw;
        }
        cv.reset_clip();
        cv.fill_rect(0.0, by, lw, bh, tk.color, ga);
        self.text.line(cv, F_BOLD, lp, label, lw / 2.0, by + 12.0 * k, Align::Center, Paint::plain(INK, ga));
    }

    fn caption_layout(&self, c: &Caption, w: f32, k: f32) -> Option<Laid> {
        if c.lines.is_empty() {
            return None;
        }
        let px = if c.large { 38.0 } else { 28.0 } * k;
        let start = c.lines.len().saturating_sub(2);
        let text = c.lines[start..].join("\n");
        let laid = self.text.layout(F_SEMI, px, &text, w * 0.76);
        if laid.lines.len() > 3 {
            let last = c.lines.last().map(|s| s.as_str()).unwrap_or("");
            return Some(self.text.layout(F_SEMI, px, last, w * 0.76));
        }
        Some(laid)
    }

    fn caption(&mut self, cv: &mut Canvas, c: &Caption, t: f64) {
        let mut a = ease(c.show.amount(t, 0.3)) * self.ov_alpha;
        if c.show.active(t, 0.3) {
            self.animating = true;
        }
        if c.auto_clear > 0.0 {
            let age = (t - c.last_t) as f32 - c.auto_clear;
            if age > 0.0 {
                a *= 1.0 - (age / 0.4).clamp(0.0, 1.0);
            }
        }
        if a <= 0.0 {
            return;
        }
        let (w, h) = (cv.w as f32, cv.h as f32);
        let k = h / 720.0;
        let Some(laid) = self.caption_layout(c, w, k) else { return };
        let bw = laid.width() + 40.0 * k;
        let bh = laid.height() + 22.0 * k;
        let bottom = h - 26.0 * k - self.ticker_h;
        let bx = (w - bw) / 2.0;
        let by = bottom - bh;
        cv.round_rect(bx, by, bw, bh, 8.0 * k, [0, 0, 0], 0.78 * a);
        self.text.draw(cv, &laid, bx + 20.0 * k, by + 9.0 * k, bw - 40.0 * k, Align::Center, Paint::plain([255, 255, 255], a));
    }

    fn logo(&mut self, cv: &mut Canvas, lg: &LogoBug, t: f64, live: bool, images: &ImageStore) {
        let a = ease(lg.show.amount(t, ODUR)) * self.gfx_alpha;
        if lg.show.active(t, ODUR) && a < 1.0 {
            self.animating = true;
        }
        if a <= 0.0 {
            return;
        }
        let (w, h) = (cv.w as f32, cv.h as f32);
        let k = h / 720.0;
        let right = w - w * 0.035;
        let top = h * 0.05;
        let mut left_edge = right;
        if let Some(p) = lg.image.as_ref().filter(|p| !p.is_empty()) {
            if let Some(img) = self.prep(images, p, Fit::Logo) {
                let x = right - img.w as f32;
                cv.blit(&img.data, img.w, img.h, x.round() as i32, top.round() as i32, a * 0.92, img.alpha);
                left_edge = x;
            }
        } else if !lg.text.is_empty() {
            let wdt = self.text.line(cv, F_SERIF, 24.0 * k, &lg.text, right, top, Align::Right, Paint::shadowed([255, 255, 255], a * 0.92));
            left_edge = right - wdt;
        }
        if live && lg.live_badge {
            let px = 15.0 * k;
            let bw = self.text.measure(F_BOLD, px, "LIVE") + 36.0 * k;
            let bx = left_edge - bw - 14.0 * k;
            cv.round_rect(bx, top + 2.0 * k, bw, 28.0 * k, 6.0 * k, [229, 72, 77], a);
            let pulse = 0.6 + 0.4 * ((t * 3.0).sin() as f32).abs();
            cv.round_rect(bx + 10.0 * k, top + 12.0 * k, 8.0 * k, 8.0 * k, 4.0 * k, [255, 255, 255], a * pulse);
            self.text.line(cv, F_BOLD, px, "LIVE", bx + 24.0 * k, top + 6.0 * k, Align::Left, Paint::plain([255, 255, 255], a));
            self.animating = true;
        }
    }
}

pub fn countdown_text(secs: f64) -> String {
    if secs <= 0.0 {
        return "Starting now".into();
    }
    let s = secs.ceil() as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioEngine;
    use std::sync::RwLock;

    fn save(cv: &Canvas, name: &str) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shots");
        let _ = std::fs::create_dir_all(&dir);
        image::RgbaImage::from_raw(cv.w as u32, cv.h as u32, cv.px.clone()).unwrap().save(dir.join(name)).unwrap();
    }

    #[test]
    fn renders_scenes() {
        let ff = Arc::new(RwLock::new(None));
        let audio = AudioEngine::new(ff.clone());
        let videos = VideoHub::new(ff, audio);
        let images = ImageStore::new();
        let mut r = Renderer::new();
        let mut cv = Canvas::new(1280, 720);
        let t = now() + 100.0;
        let mut st = LiveState::default();
        st.mirror = false;
        let verse = Slide::text(SlideKind::Scripture, "For God so loved the world, that he gave his only born Son, that whoever believes in him should not perish, but have eternal life.", "John 3:16 WEB");
        st.outputs[0].set_slide(Some(verse.clone()), Transition::Cut);
        st.outputs[1].set_slide(Some(verse), Transition::Cut);
        st.outputs[0].slide_t0 = -50.0;
        st.outputs[1].slide_t0 = -50.0;
        r.compose(&mut cv, &st, OutputId::Screen, t, &images, &videos);
        save(&cv, "scene_screen_verse.png");
        st.is_live = true;
        for s in [&mut st.lower_third.show, &mut st.logo.show, &mut st.ticker.show, &mut st.title.show] {
            s.visible = true;
            s.t0 = -50.0;
        }
        st.caption.lines = vec!["Please open your Bibles to John chapter 3.".into()];
        st.caption.show.visible = true;
        st.caption.show.t0 = -50.0;
        st.caption.last_t = t;
        r.compose(&mut cv, &st, OutputId::Stream, t, &images, &videos);
        save(&cv, "scene_stream_overlays.png");
        st.brk.show.visible = true;
        st.brk.show.t0 = t - 1.0;
        st.brk.ends_at = Some(t + 272.0);
        r.compose(&mut cv, &st, OutputId::Stream, t, &images, &videos);
        save(&cv, "scene_break.png");
        st.brk.show.visible = false;
        st.brk.show.t0 = -50.0;
        for s in [&mut st.lower_third.show, &mut st.ticker.show, &mut st.caption.show] {
            s.visible = false;
        }
        st.title.centered = true;
        let song = Slide::text(SlideKind::Lyrics, "Amazing grace! How sweet the sound\nThat saved a wretch like me!\nI once was lost, but now am found;\nWas blind, but now I see.", "Amazing Grace · John Newton");
        st.outputs[0].set_slide(Some(song), Transition::Cut);
        st.outputs[0].slide_t0 = -50.0;
        st.title.show.visible = false;
        r.compose(&mut cv, &st, OutputId::Screen, t, &images, &videos);
        save(&cv, "scene_screen_lyrics.png");
        st.lower_third.style = LtStyle::Clean;
        st.lower_third.show.visible = true;
        st.title.show.visible = true;
        r.compose(&mut cv, &st, OutputId::Stream, t, &images, &videos);
        save(&cv, "scene_stream_title.png");
        assert!(cv.px.iter().step_by(4).any(|&v| v > 200));
    }

    #[test]
    fn renders_layouts() {
        let dir = std::env::temp_dir().join("lampstand-layouts");
        let _ = std::fs::create_dir_all(&dir);
        let a = dir.join("a.png");
        let b = dir.join("b.png");
        image::RgbaImage::from_fn(640, 360, |x, y| image::Rgba([(x / 3) as u8, 90, (y / 2) as u8, 255])).save(&a).unwrap();
        image::RgbaImage::from_fn(640, 360, |x, y| if (x / 40 + y / 40) % 2 == 0 { image::Rgba([40, 140, 220, 255]) } else { image::Rgba([240, 240, 240, 255]) }).save(&b).unwrap();
        let ff = Arc::new(RwLock::new(None));
        let audio = AudioEngine::new(ff.clone());
        let videos = VideoHub::new(ff, audio);
        let images = ImageStore::new();
        let mut r = Renderer::new();
        let mut cv = Canvas::new(1280, 720);
        let mut st = LiveState::default();
        let o = &mut st.outputs[0];
        o.background = Background::Image(a.to_string_lossy().to_string());
        o.look.slow_zoom = false;
        o.set_slide(Some(Slide::text(SlideKind::Scripture, "The Lord is my shepherd; I shall lack nothing.", "Psalm 23:1 WEB")), Transition::Cut);
        o.slide_t0 = -50.0;
        o.set_pip(Some(Background::Image(b.to_string_lossy().to_string())), PipLayout::Corner);
        o.pip_t0 = -50.0;
        let t = now() + 100.0;
        for _ in 0..40 {
            r.compose(&mut cv, &st, OutputId::Screen, t, &images, &videos);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        save(&cv, "layout_corner.png");
        st.outputs[0].pip_layout = PipLayout::Side;
        st.outputs[0].look.layout = TextLayout::LowerThird;
        r.compose(&mut cv, &st, OutputId::Screen, t, &images, &videos);
        save(&cv, "layout_side.png");
        st.outputs[0].clean = true;
        st.outputs[0].clean_t0 = -50.0;
        st.outputs[0].pip = None;
        st.outputs[0].pip_layout = PipLayout::Off;
        st.logo.show.visible = true;
        st.logo.show.t0 = -50.0;
        r.compose(&mut cv, &st, OutputId::Screen, t, &images, &videos);
        save(&cv, "layout_clear.png");
        st.outputs[0].clean = false;
        st.logo.show.visible = false;
        st.outputs[0].look.layout = TextLayout::SideLeft;
        r.compose(&mut cv, &st, OutputId::Screen, t, &images, &videos);
        save(&cv, "layout_slide_side.png");
        st.outputs[0].look.layout = TextLayout::Transparent;
        st.outputs[0].set_pip(Some(Background::Image(b.to_string_lossy().to_string())), PipLayout::Corner);
        st.outputs[0].pip_t0 = -50.0;
        st.outputs[0].pip_size = 0.48;
        st.outputs[0].pip_corner = 2;
        r.compose(&mut cv, &st, OutputId::Screen, t, &images, &videos);
        save(&cv, "layout_transparent_corner.png");
        st.outputs[0].look.layout = TextLayout::Cover;
        r.compose(&mut cv, &st, OutputId::Screen, t, &images, &videos);
        save(&cv, "layout_cover.png");
    }
}
