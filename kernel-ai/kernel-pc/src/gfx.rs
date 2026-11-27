//! Grafica: buffer di disegno in RAM con cache, copiato nel framebuffer del firmware.
//! Rettangoli arrotondati con bordi sfumati, ombre, sfumature, testo con antialiasing.

use alloc::vec;
use alloc::vec::Vec;

use crate::plat::Fb;

pub struct Font {
    data: &'static [u8],
    pub ascent: i32,
    pub line: i32,
    index: Vec<(u32, usize)>, // (codepoint, offset del glifo)
}

pub static FONT_BIN: &[u8] = include_bytes!("font14.bin");
pub static FONTB_BIN: &[u8] = include_bytes!("font14b.bin");
pub static FONTL_BIN: &[u8] = include_bytes!("font30b.bin");
pub static MONO_BIN: &[u8] = include_bytes!("mono13.bin");

impl Font {
    pub fn load(data: &'static [u8]) -> Font {
        let n = u16::from_le_bytes([data[4], data[5]]) as usize;
        let mut index = Vec::with_capacity(n);
        let mut o = 8;
        for _ in 0..n {
            let cp = u32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
            let (w, h) = (data[o + 4] as usize, data[o + 5] as usize);
            index.push((cp, o));
            o += 9 + w * h;
        }
        Font { data, ascent: data[6] as i32, line: data[7] as i32, index }
    }

    fn glyph(&self, c: char) -> Option<usize> {
        let cp = c as u32;
        self.index.iter().find(|g| g.0 == cp).map(|g| g.1).or_else(|| self.index.iter().find(|g| g.0 == '?' as u32).map(|g| g.1))
    }

    pub fn width(&self, s: &str) -> i32 {
        s.chars().map(|c| self.glyph(c).map(|o| self.data[o + 8] as i32).unwrap_or(0)).sum()
    }
}

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
    /// copia dello sfondo sotto il campo di richiesta: (x, y, w, h, pixel)
    pub under: (i32, i32, i32, i32, Vec<u32>),
}

#[inline]
pub fn rgb(r: u32, g: u32, b: u32) -> u32 {
    (r << 16) | (g << 8) | b
}

#[inline]
pub fn mix(a: u32, b: u32, t: u32) -> u32 {
    // t in 0..=255: 0 = a, 255 = b
    let it = 255 - t;
    let r = (((a >> 16) & 255) * it + ((b >> 16) & 255) * t) / 255;
    let g = (((a >> 8) & 255) * it + ((b >> 8) & 255) * t) / 255;
    let bl = ((a & 255) * it + (b & 255) * t) / 255;
    (r << 16) | (g << 8) | bl
}

pub fn lighten(c: u32, t: u32) -> u32 {
    mix(c, 0xffffff, t)
}
pub fn darken(c: u32, t: u32) -> u32 {
    mix(c, 0x000000, t)
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas { w: w as i32, h: h as i32, px: vec![0; w * h], under: (0, 0, 0, 0, Vec::new()) }
    }

    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: u32, a: u32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || a == 0 {
            return;
        }
        let i = (y * self.w + x) as usize;
        self.px[i] = if a >= 255 { c } else { mix(self.px[i], c, a) };
    }

    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.w);
        let y1 = (y + h).min(self.h);
        for yy in y0..y1 {
            let row = (yy * self.w) as usize;
            self.px[row + x0 as usize..row + x1.max(x0) as usize].fill(c);
        }
    }

    pub fn fill_a(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32, a: u32) {
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                self.blend(xx, yy, c, a);
            }
        }
    }

    pub fn vgradient(&mut self, x: i32, y: i32, w: i32, h: i32, top: u32, bottom: u32) {
        for yy in 0..h {
            let t = (yy * 255 / h.max(1)) as u32;
            self.fill(x, y + yy, w, 1, mix(top, bottom, t));
        }
    }

    /// copertura (0..255) di un rettangolo arrotondato nel punto (px, py)
    fn rr_cov(px: i32, py: i32, x: i32, y: i32, w: i32, h: i32, r: i32) -> u32 {
        if r <= 0 {
            return 255;
        }
        let cx = if px < x + r { x + r } else if px >= x + w - r { x + w - r - 1 } else { return 255 };
        let cy = if py < y + r { y + r } else if py >= y + h - r { y + h - r - 1 } else { return 255 };
        let dx = (px - cx) as f32 + 0.0;
        let dy = (py - cy) as f32 + 0.0;
        let d = libm::sqrtf(dx * dx + dy * dy) - (r as f32 - 0.5);
        if d <= -0.5 {
            255
        } else if d >= 0.5 {
            0
        } else {
            ((0.5 - d) * 255.0) as u32
        }
    }

    pub fn rrect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: u32, a: u32) {
        let r = r.min(w / 2).min(h / 2);
        for yy in y.max(0)..(y + h).min(self.h) {
            let edge = yy < y + r || yy >= y + h - r;
            if !edge && a >= 255 {
                let x0 = x.max(0);
                let x1 = (x + w).min(self.w);
                if x1 > x0 {
                    let row = (yy * self.w) as usize;
                    self.px[row + x0 as usize..row + x1 as usize].fill(c);
                }
                continue;
            }
            for xx in x.max(0)..(x + w).min(self.w) {
                let cov = if edge || xx < x + r || xx >= x + w - r { Self::rr_cov(xx, yy, x, y, w, h, r) } else { 255 };
                self.blend(xx, yy, c, cov * a / 255);
            }
        }
    }

    pub fn rrect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: u32, a: u32) {
        let r = r.min(w / 2).min(h / 2);
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                let outer = Self::rr_cov(xx, yy, x, y, w, h, r);
                let inner = Self::rr_cov(xx, yy, x + 1, y + 1, w - 2, h - 2, (r - 1).max(0));
                let inside_inner = xx > x && xx < x + w - 1 && yy > y && yy < y + h - 1;
                let cov = outer.saturating_sub(if inside_inner { inner } else { 0 });
                self.blend(xx, yy, c, cov * a / 255);
            }
        }
    }

    /// ombra morbida: anelli concentrici sempre più trasparenti
    pub fn shadow(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, spread: i32, strength: u32) {
        for i in (1..=spread).rev() {
            let a = strength * (spread - i + 1) as u32 / (spread as u32 * 3);
            self.rrect(x - i, y - i + spread / 3, w + 2 * i, h + 2 * i, r + i, 0x000000, a.max(1));
        }
    }

    pub fn circle(&mut self, cx: i32, cy: i32, r: i32, c: u32, a: u32) {
        self.rrect(cx - r, cy - r, 2 * r, 2 * r, r, c, a);
    }

    pub fn text(&mut self, f: &Font, x: i32, y: i32, s: &str, c: u32) -> i32 {
        let mut pen = x;
        for ch in s.chars() {
            if let Some(o) = f.glyph(ch) {
                let d = f.data;
                let (w, h) = (d[o + 4] as i32, d[o + 5] as i32);
                let (xo, yo) = (d[o + 6] as i8 as i32, d[o + 7] as i8 as i32);
                let adv = d[o + 8] as i32;
                let bm = &d[o + 9..o + 9 + (w * h) as usize];
                for gy in 0..h {
                    for gx in 0..w {
                        let a = bm[(gy * w + gx) as usize] as u32;
                        if a != 0 {
                            self.blend(pen + xo + gx, y + yo + gy, c, a);
                        }
                    }
                }
                pen += adv;
            }
        }
        pen
    }

    /// testo troncato con «…» se supera `maxw`
    pub fn text_fit(&mut self, f: &Font, x: i32, y: i32, s: &str, c: u32, maxw: i32) {
        if f.width(s) <= maxw {
            self.text(f, x, y, s, c);
            return;
        }
        let ell = f.width("…");
        let mut w = 0;
        let mut cut = 0;
        for (i, ch) in s.char_indices() {
            let mut b = [0u8; 4];
            let cw = f.width(ch.encode_utf8(&mut b));
            if w + cw + ell > maxw {
                break;
            }
            w += cw;
            cut = i + ch.len_utf8();
        }
        let px = self.text(f, x, y, &s[..cut], c);
        self.text(f, px, y, "…", c);
    }

    pub fn save_under(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let mut v = Vec::with_capacity((w * h).max(0) as usize);
        for yy in y..y + h {
            for xx in x..x + w {
                v.push(if xx >= 0 && yy >= 0 && xx < self.w && yy < self.h { self.px[(yy * self.w + xx) as usize] } else { 0 });
            }
        }
        self.under = (x, y, w, h, v);
    }

    pub fn restore_under(&mut self) {
        let (x, y, w, h) = (self.under.0, self.under.1, self.under.2, self.under.3);
        let v = core::mem::take(&mut self.under.4);
        for yy in 0..h {
            for xx in 0..w {
                let (px, py) = (x + xx, y + yy);
                if px >= 0 && py >= 0 && px < self.w && py < self.h {
                    self.px[(py * self.w + px) as usize] = v[(yy * w + xx) as usize];
                }
            }
        }
        self.under.4 = v;
    }

    /// copia nel framebuffer la regione indicata (tutto se w == 0)
    pub fn present(&self, fb: &Fb, x: i32, y: i32, w: i32, h: i32) {
        let (x, y, w, h) = if w == 0 { (0, 0, self.w, self.h) } else { (x, y, w, h) };
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = ((x + w).min(self.w) as usize).min(fb.w);
        let y1 = ((y + h).min(self.h) as usize).min(fb.h);
        if x1 <= x0 {
            return;
        }
        for yy in y0..y1 {
            let src = &self.px[yy * self.w as usize + x0..yy * self.w as usize + x1];
            unsafe {
                let dst = (fb.ptr as *mut u8).add(yy * fb.pitch + x0 * 4) as *mut u32;
                if fb.rgb {
                    for (i, &p) in src.iter().enumerate() {
                        let q = ((p & 0xff) << 16) | (p & 0xff00) | ((p >> 16) & 0xff);
                        core::ptr::write_volatile(dst.add(i), q);
                    }
                } else {
                    core::ptr::copy_nonoverlapping(src.as_ptr(), dst, x1 - x0);
                }
            }
        }
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    }
}
