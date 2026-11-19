use crate::model::Rgb;

#[derive(Clone, Copy, Debug)]
pub struct Clip {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u8>,
    pub clip: Clip,
}

#[inline]
fn mix(d: u8, s: u8, a: u32) -> u8 {
    let d = d as u32;
    let s = s as u32;
    ((d * (256 - a) + s * a) >> 8) as u8
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        let (w, h) = (w as i32, h as i32);
        Canvas { w, h, px: vec![0; (w * h * 4) as usize], clip: Clip { x0: 0, y0: 0, x1: w, y1: h } }
    }

    pub fn reset_clip(&mut self) {
        self.clip = Clip { x0: 0, y0: 0, x1: self.w, y1: self.h };
    }

    pub fn set_clip(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.clip = Clip {
            x0: (x0.round() as i32).clamp(0, self.w),
            y0: (y0.round() as i32).clamp(0, self.h),
            x1: (x1.round() as i32).clamp(0, self.w),
            y1: (y1.round() as i32).clamp(0, self.h),
        };
    }

    pub fn clear(&mut self, c: Rgb) {
        for p in self.px.chunks_exact_mut(4) {
            p[0] = c[0];
            p[1] = c[1];
            p[2] = c[2];
            p[3] = 255;
        }
    }

    fn span(&self, x: f32, y: f32, w: f32, h: f32) -> Option<(i32, i32, i32, i32)> {
        let x0 = (x.round() as i32).max(self.clip.x0);
        let y0 = (y.round() as i32).max(self.clip.y0);
        let x1 = ((x + w).round() as i32).min(self.clip.x1);
        let y1 = ((y + h).round() as i32).min(self.clip.y1);
        if x1 <= x0 || y1 <= y0 { None } else { Some((x0, y0, x1, y1)) }
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb, alpha: f32) {
        let a = (alpha.clamp(0.0, 1.0) * 256.0) as u32;
        if a == 0 {
            return;
        }
        let Some((x0, y0, x1, y1)) = self.span(x, y, w, h) else { return };
        let stride = self.w as usize * 4;
        for yy in y0..y1 {
            let row = &mut self.px[yy as usize * stride + x0 as usize * 4..yy as usize * stride + x1 as usize * 4];
            if a >= 256 {
                for p in row.chunks_exact_mut(4) {
                    p[0] = c[0];
                    p[1] = c[1];
                    p[2] = c[2];
                }
            } else {
                for p in row.chunks_exact_mut(4) {
                    p[0] = mix(p[0], c[0], a);
                    p[1] = mix(p[1], c[1], a);
                    p[2] = mix(p[2], c[2], a);
                }
            }
        }
    }

    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, alpha: f32) {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        if r < 1.0 {
            self.fill_rect(x, y, w, h, c, alpha);
            return;
        }
        let Some((x0, y0, x1, y1)) = self.span(x, y, w, h) else { return };
        let stride = self.w as usize * 4;
        for yy in y0..y1 {
            let fy = yy as f32 + 0.5;
            for xx in x0..x1 {
                let fx = xx as f32 + 0.5;
                let cx = fx.clamp(x + r, (x + w - r).max(x + r));
                let cy = fy.clamp(y + r, (y + h - r).max(y + r));
                let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                let cov = (r + 0.5 - d).clamp(0.0, 1.0);
                let a = (alpha * cov * 256.0) as u32;
                if a == 0 {
                    continue;
                }
                let i = yy as usize * stride + xx as usize * 4;
                let p = &mut self.px[i..i + 4];
                p[0] = mix(p[0], c[0], a.min(256));
                p[1] = mix(p[1], c[1], a.min(256));
                p[2] = mix(p[2], c[2], a.min(256));
            }
        }
    }

    pub fn vgradient(&mut self, x: f32, y: f32, w: f32, h: f32, top: Rgb, bottom: Rgb, a_top: f32, a_bottom: f32) {
        let Some((x0, y0, x1, y1)) = self.span(x, y, w, h) else { return };
        for yy in y0..y1 {
            let t = ((yy as f32 - y) / h.max(1.0)).clamp(0.0, 1.0);
            let c = lerp_rgb(top, bottom, t);
            let a = a_top + (a_bottom - a_top) * t;
            self.fill_rect(x0 as f32, yy as f32, (x1 - x0) as f32, 1.0, c, a);
        }
    }

    pub fn hgradient(&mut self, x: f32, y: f32, w: f32, h: f32, left: Rgb, right: Rgb, a_left: f32, a_right: f32) {
        let Some((x0, y0, x1, y1)) = self.span(x, y, w, h) else { return };
        let stride = self.w as usize * 4;
        for xx in x0..x1 {
            let t = ((xx as f32 - x) / w.max(1.0)).clamp(0.0, 1.0);
            let c = lerp_rgb(left, right, t);
            let a = ((a_left + (a_right - a_left) * t).clamp(0.0, 1.0) * 256.0) as u32;
            if a == 0 {
                continue;
            }
            for yy in y0..y1 {
                let i = yy as usize * stride + xx as usize * 4;
                let p = &mut self.px[i..i + 4];
                p[0] = mix(p[0], c[0], a);
                p[1] = mix(p[1], c[1], a);
                p[2] = mix(p[2], c[2], a);
            }
        }
    }

    pub fn diagonal_gradient(&mut self, a: Rgb, b: Rgb) {
        let (w, h) = (self.w as f32, self.h as f32);
        let stride = self.w as usize * 4;
        for yy in 0..self.h {
            for xx in 0..self.w {
                let t = ((xx as f32 / w) * 0.55 + (yy as f32 / h) * 0.45).clamp(0.0, 1.0);
                let c = lerp_rgb(a, b, t);
                let i = yy as usize * stride + xx as usize * 4;
                self.px[i] = c[0];
                self.px[i + 1] = c[1];
                self.px[i + 2] = c[2];
                self.px[i + 3] = 255;
            }
        }
    }

    pub fn mask(&mut self, x: i32, y: i32, mw: i32, mh: i32, cov: &[u8], c: Rgb, alpha: f32) {
        let a = (alpha.clamp(0.0, 1.0) * 256.0) as u32;
        if a == 0 {
            return;
        }
        let x0 = x.max(self.clip.x0);
        let y0 = y.max(self.clip.y0);
        let x1 = (x + mw).min(self.clip.x1);
        let y1 = (y + mh).min(self.clip.y1);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let stride = self.w as usize * 4;
        for yy in y0..y1 {
            let my = (yy - y) as usize * mw as usize;
            for xx in x0..x1 {
                let m = cov[my + (xx - x) as usize] as u32;
                if m == 0 {
                    continue;
                }
                let k = (m * a) >> 8;
                let i = yy as usize * stride + xx as usize * 4;
                let p = &mut self.px[i..i + 4];
                p[0] = mix(p[0], c[0], k);
                p[1] = mix(p[1], c[1], k);
                p[2] = mix(p[2], c[2], k);
            }
        }
    }

    pub fn blit(&mut self, src: &[u8], sw: i32, sh: i32, dx: i32, dy: i32, alpha: f32, src_has_alpha: bool) {
        let a = (alpha.clamp(0.0, 1.0) * 256.0) as u32;
        if a == 0 || src.len() < (sw * sh * 4) as usize {
            return;
        }
        let x0 = dx.max(self.clip.x0);
        let y0 = dy.max(self.clip.y0);
        let x1 = (dx + sw).min(self.clip.x1);
        let y1 = (dy + sh).min(self.clip.y1);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let stride = self.w as usize * 4;
        let sstride = sw as usize * 4;
        for yy in y0..y1 {
            let drow = yy as usize * stride;
            let srow = (yy - dy) as usize * sstride;
            let d = &mut self.px[drow + x0 as usize * 4..drow + x1 as usize * 4];
            let s = &src[srow + (x0 - dx) as usize * 4..srow + (x1 - dx) as usize * 4];
            if a >= 256 && !src_has_alpha {
                d.copy_from_slice(s);
                continue;
            }
            for (dp, sp) in d.chunks_exact_mut(4).zip(s.chunks_exact(4)) {
                let k = if src_has_alpha { (sp[3] as u32 * a) >> 8 } else { a };
                if k == 0 {
                    continue;
                }
                dp[0] = mix(dp[0], sp[0], k);
                dp[1] = mix(dp[1], sp[1], k);
                dp[2] = mix(dp[2], sp[2], k);
            }
        }
        if a >= 256 && !src_has_alpha {
            for yy in y0..y1 {
                let drow = yy as usize * stride;
                for xx in x0..x1 {
                    self.px[drow + xx as usize * 4 + 3] = 255;
                }
            }
        }
    }

    pub fn blit_scaled(&mut self, src: &[u8], sw: i32, sh: i32, x: f32, y: f32, w: f32, h: f32, alpha: f32, src_has_alpha: bool) {
        let a = (alpha.clamp(0.0, 1.0) * 256.0) as u32;
        if a == 0 || w < 1.0 || h < 1.0 || sw < 2 || sh < 2 {
            return;
        }
        let Some((x0, y0, x1, y1)) = self.span(x, y, w, h) else { return };
        let stride = self.w as usize * 4;
        let sstride = sw as usize * 4;
        let fx = (sw as f32) / w;
        let fy = (sh as f32) / h;
        if fx >= 1.6 && fy >= 1.6 {
            let bx = (fx.ceil() as i32).clamp(2, 4);
            let by = (fy.ceil() as i32).clamp(2, 4);
            let n = (bx * by) as u32;
            for yy in y0..y1 {
                let sy = (((yy as f32 - y) * fy) as i32).clamp(0, sh - by);
                for xx in x0..x1 {
                    let sx = (((xx as f32 - x) * fx) as i32).clamp(0, sw - bx);
                    let mut acc = [0u32; 4];
                    for j in 0..by {
                        let row = ((sy + j) * sw + sx) as usize * 4;
                        for i in 0..bx as usize {
                            let q = row + i * 4;
                            acc[0] += src[q] as u32;
                            acc[1] += src[q + 1] as u32;
                            acc[2] += src[q + 2] as u32;
                            acc[3] += src[q + 3] as u32;
                        }
                    }
                    let k = if src_has_alpha { ((acc[3] / n) * a) >> 8 } else { a };
                    if k == 0 {
                        continue;
                    }
                    let i = yy as usize * stride + xx as usize * 4;
                    let p = &mut self.px[i..i + 4];
                    p[0] = mix(p[0], (acc[0] / n) as u8, k.min(256));
                    p[1] = mix(p[1], (acc[1] / n) as u8, k.min(256));
                    p[2] = mix(p[2], (acc[2] / n) as u8, k.min(256));
                }
            }
            return;
        }
        let maxx = (sw - 2) as f32;
        let maxy = (sh - 2) as f32;
        for yy in y0..y1 {
            let sy = ((yy as f32 + 0.5 - y) * fy - 0.5).clamp(0.0, maxy);
            let iy = sy as usize;
            let ty = ((sy - iy as f32) * 256.0) as u32;
            let r0 = iy * sstride;
            let r1 = r0 + sstride;
            for xx in x0..x1 {
                let sx = ((xx as f32 + 0.5 - x) * fx - 0.5).clamp(0.0, maxx);
                let ix = sx as usize;
                let tx = ((sx - ix as f32) * 256.0) as u32;
                let i00 = r0 + ix * 4;
                let i10 = i00 + 4;
                let i01 = r1 + ix * 4;
                let i11 = i01 + 4;
                let mut out = [0u32; 4];
                for c in 0..4 {
                    let top = src[i00 + c] as u32 * (256 - tx) + src[i10 + c] as u32 * tx;
                    let bot = src[i01 + c] as u32 * (256 - tx) + src[i11 + c] as u32 * tx;
                    out[c] = (top * (256 - ty) + bot * ty) >> 16;
                }
                let k = if src_has_alpha { (out[3] * a) >> 8 } else { a };
                if k == 0 {
                    continue;
                }
                let i = yy as usize * stride + xx as usize * 4;
                let p = &mut self.px[i..i + 4];
                p[0] = mix(p[0], out[0] as u8, k.min(256));
                p[1] = mix(p[1], out[1] as u8, k.min(256));
                p[2] = mix(p[2], out[2] as u8, k.min(256));
            }
        }
    }

    pub fn darken(&mut self, mask: &[u8], strength: f32) {
        let s = (strength.clamp(0.0, 1.0) * 256.0) as u32;
        if s == 0 || mask.len() * 4 != self.px.len() {
            return;
        }
        for (p, m) in self.px.chunks_exact_mut(4).zip(mask.iter()) {
            let k = 256 - ((*m as u32 * s) >> 8);
            p[0] = ((p[0] as u32 * k) >> 8) as u8;
            p[1] = ((p[1] as u32 * k) >> 8) as u8;
            p[2] = ((p[2] as u32 * k) >> 8) as u8;
        }
    }
}

pub fn lerp_rgb(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2])]
}

pub fn shade(c: Rgb, f: f32) -> Rgb {
    [(c[0] as f32 * f).min(255.0) as u8, (c[1] as f32 * f).min(255.0) as u8, (c[2] as f32 * f).min(255.0) as u8]
}

pub fn vignette_mask(w: u32, h: u32) -> Vec<u8> {
    let (fw, fh) = (w as f32, h as f32);
    let mut m = vec![0u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 / fw - 0.5) * 2.0;
            let dy = (y as f32 / fh - 0.5) * 2.0;
            let d = (dx * dx * 0.8 + dy * dy).sqrt();
            let v = ((d - 0.55) / 0.9).clamp(0.0, 1.0);
            m[(y * w + x) as usize] = (v * v * 255.0) as u8;
        }
    }
    m
}

#[cfg(test)]
mod icon {
    use super::*;

    #[test]
    #[ignore]
    fn writes_app_icon() {
        let n = 1024u32;
        let mut cv = Canvas::new(n, n);
        let f = n as f32;
        cv.diagonal_gradient([24, 26, 48], [86, 44, 78]);
        let gold = [232, 184, 86];
        let pale = [255, 236, 190];
        let cx = f / 2.0;
        for i in 0..40 {
            let t = i as f32 / 40.0;
            let r = f * (0.19 - 0.13 * t);
            let y = f * (0.36 - 0.2 * t);
            cv.round_rect(cx - r / 2.0, y - r / 2.0, r, r, r / 2.0, lerp_rgb(gold, pale, t * 0.6), 0.9);
        }
        cv.round_rect(cx - f * 0.035, f * 0.3, f * 0.07, f * 0.09, f * 0.035, [255, 250, 235], 0.95);
        cv.round_rect(cx - f * 0.16, f * 0.47, f * 0.32, f * 0.05, f * 0.025, gold, 1.0);
        cv.round_rect(cx - f * 0.04, f * 0.5, f * 0.08, f * 0.22, f * 0.02, gold, 1.0);
        cv.round_rect(cx - f * 0.2, f * 0.72, f * 0.4, f * 0.06, f * 0.03, gold, 1.0);
        let margin = f * 0.09;
        let side = f - margin * 2.0;
        let radius = side * 0.225;
        for y in 0..n {
            for x in 0..n {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let qx = fx.clamp(margin + radius, (margin + side - radius).max(margin + radius));
                let qy = fy.clamp(margin + radius, (margin + side - radius).max(margin + radius));
                let d = ((fx - qx).powi(2) + (fy - qy).powi(2)).sqrt();
                let a = (radius + 0.5 - d).clamp(0.0, 1.0);
                let inside = fx >= margin && fx <= margin + side && fy >= margin && fy <= margin + side;
                let i = ((y * n + x) * 4 + 3) as usize;
                cv.px[i] = if inside { (a * 255.0) as u8 } else { 0 };
            }
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("macos").join("icon_1024.png");
        image::RgbaImage::from_raw(n, n, cv.px).unwrap().save(path).unwrap();
    }
}
