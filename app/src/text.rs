use crate::canvas::Canvas;
use crate::model::{FontChoice, Rgb};
use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont};
use std::collections::HashMap;
use std::sync::Arc;

pub const SERIF: &[u8] = include_bytes!("../assets/fonts/PlayfairDisplay-var.ttf");
pub const SANS: &[u8] = include_bytes!("../assets/fonts/Figtree-Regular.ttf");
pub const SANS_SEMI: &[u8] = include_bytes!("../assets/fonts/Figtree-SemiBold.ttf");
pub const SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/Figtree-Bold.ttf");

pub struct Glyph {
    w: i32,
    h: i32,
    left: i32,
    top: i32,
    cov: Vec<u8>,
}

pub struct LaidLine {
    pub glyphs: Vec<(GlyphId, f32)>,
    pub width: f32,
}

pub struct Laid {
    pub font: usize,
    pub px: f32,
    pub lines: Vec<LaidLine>,
    pub line_h: f32,
    pub ascent: f32,
}

impl Laid {
    pub fn height(&self) -> f32 {
        if self.lines.is_empty() { 0.0 } else { self.line_h * (self.lines.len() as f32 - 1.0) + self.ascent * 1.25 }
    }
    pub fn width(&self) -> f32 {
        self.lines.iter().map(|l| l.width).fold(0.0, f32::max)
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy)]
pub struct Paint {
    pub color: Rgb,
    pub alpha: f32,
    pub shadow: bool,
    pub outline: bool,
}

impl Paint {
    pub fn plain(color: Rgb, alpha: f32) -> Self {
        Paint { color, alpha, shadow: false, outline: false }
    }
    pub fn shadowed(color: Rgb, alpha: f32) -> Self {
        Paint { color, alpha, shadow: true, outline: false }
    }
}

pub const F_SERIF: usize = 0;
pub const F_SANS: usize = 1;
pub const F_SEMI: usize = 2;
pub const F_BOLD: usize = 3;

pub fn font_index(f: FontChoice) -> usize {
    match f {
        FontChoice::Serif => F_SERIF,
        FontChoice::Sans => F_SANS,
        FontChoice::SansBold => F_BOLD,
    }
}

pub struct TextEngine {
    fonts: Vec<FontArc>,
    cache: HashMap<(usize, u16, u16), Option<Arc<Glyph>>>,
}

impl TextEngine {
    pub fn new() -> Self {
        let fonts = [SERIF, SANS, SANS_SEMI, SANS_BOLD]
            .iter()
            .map(|b| FontArc::try_from_slice(b).expect("bundled font"))
            .collect();
        TextEngine { fonts, cache: HashMap::new() }
    }

    fn glyph(&mut self, font: usize, id: GlyphId, px: f32) -> Option<Arc<Glyph>> {
        let key = (font, id.0, px.round() as u16);
        if self.cache.len() > 6000 {
            self.cache.clear();
        }
        let f = &self.fonts[font];
        self.cache
            .entry(key)
            .or_insert_with(|| {
                let g = id.with_scale_and_position(PxScale::from(px.round()), ab_glyph::point(0.0, 0.0));
                let og = f.outline_glyph(g)?;
                let b = og.px_bounds();
                let w = b.width().ceil() as i32;
                let h = b.height().ceil() as i32;
                if w <= 0 || h <= 0 {
                    return None;
                }
                let mut cov = vec![0u8; (w * h) as usize];
                og.draw(|x, y, c| {
                    let i = (y as i32 * w + x as i32) as usize;
                    if i < cov.len() {
                        cov[i] = (c.clamp(0.0, 1.0) * 255.0) as u8;
                    }
                });
                Some(Arc::new(Glyph { w, h, left: b.min.x.floor() as i32, top: b.min.y.floor() as i32, cov }))
            })
            .clone()
    }

    pub fn measure(&self, font: usize, px: f32, s: &str) -> f32 {
        let f = self.fonts[font].as_scaled(PxScale::from(px));
        let mut w = 0.0;
        let mut prev: Option<GlyphId> = None;
        for c in s.chars() {
            let id = f.glyph_id(c);
            if let Some(p) = prev {
                w += f.kern(p, id);
            }
            w += f.h_advance(id);
            prev = Some(id);
        }
        w
    }

    pub fn layout(&self, font: usize, px: f32, text: &str, max_w: f32) -> Laid {
        let sf = self.fonts[font].as_scaled(PxScale::from(px));
        let space = sf.h_advance(sf.glyph_id(' '));
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let words: Vec<&str> = para.split_whitespace().collect();
            if words.is_empty() {
                lines.push(LaidLine { glyphs: vec![], width: 0.0 });
                continue;
            }
            let mut cur: Vec<&str> = Vec::new();
            let mut cur_w = 0.0;
            for w in words {
                let ww = self.measure(font, px, w);
                let add = if cur.is_empty() { ww } else { space + ww };
                if !cur.is_empty() && cur_w + add > max_w {
                    lines.push(self.shape_line(font, px, &cur.join(" ")));
                    cur.clear();
                    cur.push(w);
                    cur_w = ww;
                } else {
                    cur.push(w);
                    cur_w += add;
                }
            }
            if !cur.is_empty() {
                lines.push(self.shape_line(font, px, &cur.join(" ")));
            }
        }
        let line_h = (sf.ascent() - sf.descent() + sf.line_gap()) * if font == F_SERIF { 1.0 } else { 1.12 };
        Laid { font, px, lines, line_h, ascent: sf.ascent() }
    }

    fn shape_line(&self, font: usize, px: f32, s: &str) -> LaidLine {
        let f = self.fonts[font].as_scaled(PxScale::from(px));
        let mut x = 0.0;
        let mut prev: Option<GlyphId> = None;
        let mut glyphs = Vec::with_capacity(s.len());
        for c in s.chars() {
            let id = f.glyph_id(c);
            if let Some(p) = prev {
                x += f.kern(p, id);
            }
            glyphs.push((id, x));
            x += f.h_advance(id);
            prev = Some(id);
        }
        LaidLine { glyphs, width: x }
    }

    pub fn fit(&self, font: usize, text: &str, max_w: f32, max_h: f32, max_px: f32, min_px: f32) -> Laid {
        let mut px = max_px;
        loop {
            let l = self.layout(font, px, text, max_w);
            if (l.height() <= max_h && l.width() <= max_w * 1.02) || px <= min_px {
                return l;
            }
            px = (px * 0.93).max(min_px);
        }
    }

    pub fn draw(&mut self, cv: &mut Canvas, laid: &Laid, x: f32, y: f32, box_w: f32, align: Align, paint: Paint) {
        if paint.alpha <= 0.003 {
            return;
        }
        let px = laid.px;
        if paint.shadow {
            let off = (px / 18.0).clamp(1.0, 6.0);
            self.draw_pass(cv, laid, x + off, y + off * 1.3, box_w, align, [0, 0, 0], paint.alpha * 0.55);
        }
        if paint.outline {
            let r = (px / 22.0).clamp(1.0, 4.0);
            for (dx, dy) in [(-r, 0.0), (r, 0.0), (0.0, -r), (0.0, r), (-r * 0.7, -r * 0.7), (r * 0.7, r * 0.7), (-r * 0.7, r * 0.7), (r * 0.7, -r * 0.7)] {
                self.draw_pass(cv, laid, x + dx, y + dy, box_w, align, [0, 0, 0], paint.alpha * 0.85);
            }
        }
        self.draw_pass(cv, laid, x, y, box_w, align, paint.color, paint.alpha);
    }

    fn draw_pass(&mut self, cv: &mut Canvas, laid: &Laid, x: f32, y: f32, box_w: f32, align: Align, color: Rgb, alpha: f32) {
        for (i, line) in laid.lines.iter().enumerate() {
            let lx = match align {
                Align::Left => x,
                Align::Center => x + (box_w - line.width) / 2.0,
                Align::Right => x + box_w - line.width,
            };
            let base = y + laid.ascent + i as f32 * laid.line_h;
            for &(id, gx) in &line.glyphs {
                if let Some(g) = self.glyph(laid.font, id, laid.px) {
                    let px = (lx + gx).round() as i32 + g.left;
                    let py = base.round() as i32 + g.top;
                    cv.mask(px, py, g.w, g.h, &g.cov, color, alpha);
                }
            }
        }
    }

    pub fn line(&mut self, cv: &mut Canvas, font: usize, px: f32, s: &str, x: f32, y: f32, align: Align, paint: Paint) -> f32 {
        let laid = self.layout(font, px, s, f32::MAX);
        let w = laid.width();
        let x0 = match align {
            Align::Left => x,
            Align::Center => x - w / 2.0,
            Align::Right => x - w,
        };
        self.draw(cv, &laid, x0, y, w, Align::Left, paint);
        w
    }
}
