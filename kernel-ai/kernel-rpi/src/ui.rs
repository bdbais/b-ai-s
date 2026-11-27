//! Desktop disegnato dal kernel: tema modificabile da frase, finestre, dock o barra,
//! barra dei menu, campo di richiesta. Tutto viene ridisegnato dallo stato.

use alloc::string::String;
use alloc::vec::Vec;

use crate::gfx::{darken, lighten, mix, Canvas, Font};

#[derive(Clone, Copy, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub btn_right: bool,
    pub title_center: bool,
    pub radius: i32,
    pub accent: u32,
    pub wall: (u32, u32),
    pub dock: bool,
    pub menubar: bool,
    pub shadows: bool,
    pub name: &'static str,
}

impl Theme {
    pub const BAIS: Theme = Theme {
        dark: true,
        btn_right: false,
        title_center: false,
        radius: 8,
        accent: 0xE0A43A, // ambra
        wall: (0x23262B, 0x101114),
        dock: true,
        menubar: true,
        shadows: true,
        name: "B-AI-S",
    };
    pub const MACOS: Theme = Theme {
        dark: false,
        btn_right: false,
        title_center: true,
        radius: 11,
        accent: 0x0A84FF,
        wall: (0x6C7BD8, 0xC98BB9),
        dock: true,
        menubar: true,
        shadows: true,
        name: "macOS",
    };
    pub const WIN11: Theme = Theme {
        dark: false,
        btn_right: true,
        title_center: false,
        radius: 7,
        accent: 0x0067C0,
        wall: (0x2B6CB0, 0x9CC3E6),
        dock: false,
        menubar: false,
        shadows: true,
        name: "Windows 11",
    };
    pub const MIX: Theme = Theme {
        dark: true,
        btn_right: true,
        title_center: true,
        radius: 12,
        accent: 0x9B6BFF,
        wall: (0x2D2244, 0x0F1A2A),
        dock: true,
        menubar: true,
        shadows: true,
        name: "mix",
    };

    fn win_bg(&self) -> u32 {
        if self.dark { 0x1C1E22 } else { 0xFAFAFB }
    }
    fn title_bg(&self) -> u32 {
        if self.dark { 0x2A2D33 } else { 0xE8E8EC }
    }
    pub fn fg(&self) -> u32 {
        if self.dark { 0xE6E6E6 } else { 0x1D1D1F }
    }
    pub fn dim(&self) -> u32 {
        if self.dark { 0x8D9199 } else { 0x6E6E73 }
    }
    fn bar_bg(&self) -> u32 {
        if self.dark { 0x17181B } else { 0xF2F2F5 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WinKind {
    Terminal,
    Brain,
    System,
}

impl WinKind {
    pub fn title(self) -> &'static str {
        match self {
            WinKind::Terminal => "Terminale",
            WinKind::Brain => "Brain",
            WinKind::System => "Sistema",
        }
    }
    fn letter(self) -> &'static str {
        match self {
            WinKind::Terminal => ">_",
            WinKind::Brain => "B",
            WinKind::System => "S",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Input,
    Out,
    Note,
    Err,
}

pub struct Win {
    pub kind: WinKind,
    pub open: bool,
    pub max: bool,
}

pub struct Fonts {
    pub ui: Font,
    pub bold: Font,
    pub large: Font,
    pub mono: Font,
}

pub struct Desk {
    pub theme: Theme,
    pub wins: [Win; 3],
    pub term: Vec<(String, LineKind)>,
    pub input: String,
    pub notes: Vec<String>,
    pub sys: Vec<(String, String)>,
    pub tray: String,
}

impl Desk {
    pub fn new() -> Desk {
        Desk {
            theme: Theme::BAIS,
            wins: [
                Win { kind: WinKind::Terminal, open: true, max: false },
                Win { kind: WinKind::Brain, open: true, max: false },
                Win { kind: WinKind::System, open: true, max: false },
            ],
            term: Vec::new(),
            input: String::new(),
            notes: Vec::new(),
            sys: Vec::new(),
            tray: String::new(),
        }
    }

    pub fn win(&mut self, k: WinKind) -> &mut Win {
        self.wins.iter_mut().find(|w| w.kind == k).unwrap()
    }

    pub fn say(&mut self, s: &str, k: LineKind) {
        for l in s.split('\n') {
            self.term.push((String::from(l), k));
        }
        let n = self.term.len();
        if n > 400 {
            self.term.drain(..n - 400);
        }
    }

    /// aggiunge testo all'ultima riga (per i token generati)
    pub fn append(&mut self, s: &str) {
        for (i, part) in s.split('\n').enumerate() {
            if i > 0 || self.term.is_empty() {
                self.term.push((String::new(), LineKind::Out));
            }
            if let Some(last) = self.term.last_mut() {
                last.0.push_str(part);
            }
        }
    }
}

pub struct Layout {
    pub top: i32,
    pub bottom: i32,
    pub prompt: (i32, i32, i32, i32),
}

pub fn layout(c: &Canvas, t: &Theme) -> Layout {
    let top = if t.menubar { 28 } else { 0 };
    let bar = if t.dock { 78 } else { 48 };
    let pw = (c.w - 48).min(760);
    let ph = 40;
    let py = c.h - bar - ph - 12;
    Layout { top, bottom: py - 12, prompt: ((c.w - pw) / 2, py, pw, ph) }
}

fn win_rect(c: &Canvas, l: &Layout, k: WinKind, max: bool) -> (i32, i32, i32, i32) {
    let m = 20;
    let area_h = l.bottom - l.top - m;
    if max {
        return (m, l.top + m, c.w - 2 * m, area_h - m / 2);
    }
    let tw = (c.w - 3 * m) * 58 / 100;
    let rw = c.w - 3 * m - tw;
    let bh = area_h * 46 / 100;
    match k {
        WinKind::Terminal => (m, l.top + m, tw, area_h - m / 2),
        WinKind::Brain => (2 * m + tw, l.top + m, rw, bh),
        WinKind::System => (2 * m + tw, l.top + 2 * m + bh, rw, area_h - bh - m - m / 2),
    }
}

/// righe di testo spezzate per larghezza
fn wrap(f: &Font, s: &str, maxw: i32, out: &mut Vec<String>) {
    if f.width(s) <= maxw {
        out.push(String::from(s));
        return;
    }
    let mut cur = String::new();
    for word in s.split(' ') {
        let cand_w = f.width(&cur) + f.width(" ") + f.width(word);
        if !cur.is_empty() && cand_w > maxw {
            out.push(core::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        // parola troppo lunga: spezzala a caratteri
        if f.width(word) > maxw {
            for ch in word.chars() {
                let mut b = [0u8; 4];
                if f.width(&cur) + f.width(ch.encode_utf8(&mut b)) > maxw {
                    out.push(core::mem::take(&mut cur));
                }
                cur.push(ch);
            }
        } else {
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
}

fn draw_window(c: &mut Canvas, f: &Fonts, d: &Desk, k: WinKind, r: (i32, i32, i32, i32)) {
    let t = &d.theme;
    let (x, y, w, h) = r;
    let rad = t.radius;
    if t.shadows {
        c.shadow(x, y, w, h, rad, 14, if t.dark { 150 } else { 70 });
    }
    c.rrect(x, y, w, h, rad, t.win_bg(), 255);
    // barra del titolo: parte alta arrotondata, il resto squadrato
    let th = 32;
    c.rrect(x, y, w, th, rad, t.title_bg(), 255);
    c.fill(x, y + th - rad.max(1), w, rad.max(1), t.title_bg());
    c.fill(x, y + th, w, 1, if t.dark { 0x3A3D44 } else { 0xD5D5DA });
    c.rrect_outline(x, y, w, h, rad, if t.dark { 0x40444C } else { 0xC9C9CF }, 255);

    let title = k.title();
    let tw = f.bold.width(title);
    let ty = y + 8;
    if t.btn_right {
        // stile Windows: – □ ×
        let gl = ["–", "", "×"];
        for (i, g) in gl.iter().enumerate() {
            let bx = x + w - 46 * (3 - i as i32);
            if i == 1 {
                c.rrect_outline(bx + 17, y + 11, 11, 10, 2, t.fg(), 220);
            } else {
                c.text(&f.ui, bx + 18, ty, g, t.fg());
            }
        }
        let tx = if t.title_center { x + (w - tw) / 2 } else { x + 14 };
        c.text(&f.bold, tx, ty, title, t.fg());
    } else {
        let cols = [0xFF5F57, 0xFEBC2E, 0x28C840];
        for (i, col) in cols.iter().enumerate() {
            c.circle(x + 18 + i as i32 * 20, y + 16, 6, *col, 255);
        }
        let tx = if t.title_center { x + (w - tw) / 2 } else { x + 82 };
        c.text(&f.bold, tx, ty, title, t.fg());
    }

    // contenuto
    let bx = x + 14;
    let by = y + th + 10;
    let bw = w - 28;
    let bh = h - th - 18;
    match k {
        WinKind::Terminal => {
            let lh = f.mono.line;
            let maxl = (bh / lh).max(1) as usize;
            let mut lines: Vec<(String, crate::ui::LineKind)> = Vec::new();
            // dal fondo verso l'alto finché c'è spazio
            for (s, lk) in d.term.iter().rev() {
                let mut wr = Vec::new();
                let shown = if *lk == LineKind::Input { alloc::format!("> {}", s) } else { s.clone() };
                wrap(&f.mono, &shown, bw, &mut wr);
                for l in wr.into_iter().rev() {
                    lines.push((l, *lk));
                }
                if lines.len() >= maxl {
                    break;
                }
            }
            lines.truncate(maxl);
            let mut yy = by + (maxl - lines.len()) as i32 * lh;
            for (s, lk) in lines.iter().rev() {
                let col = match lk {
                    LineKind::Input => t.accent,
                    LineKind::Out => t.fg(),
                    LineKind::Note => t.dim(),
                    LineKind::Err => 0xE5484D,
                };
                c.text(&f.mono, bx, yy, s, col);
                yy += lh;
            }
        }
        WinKind::Brain => {
            let mut yy = by;
            let n = d.notes.len();
            c.text(&f.ui, bx, yy, &alloc::format!("{} ricordi in RAM", n), t.dim());
            yy += f.ui.line + 6;
            if n == 0 {
                c.text_fit(&f.ui, bx, yy, "Vuoto. Prova: ricorda: il server è in cantina", t.dim(), bw);
            }
            for note in d.notes.iter().rev() {
                if yy + f.ui.line > by + bh {
                    break;
                }
                c.circle(bx + 3, yy + 9, 3, t.accent, 255);
                c.text_fit(&f.ui, bx + 14, yy, note, t.fg(), bw - 14);
                yy += f.ui.line + 4;
            }
        }
        WinKind::System => {
            let mut yy = by;
            let kw = (bw * 40 / 100).min(150);
            for (key, val) in &d.sys {
                if yy + f.ui.line > by + bh {
                    break;
                }
                c.text_fit(&f.ui, bx, yy, key, t.dim(), kw - 8);
                c.text_fit(&f.ui, bx + kw, yy, val, t.fg(), bw - kw);
                yy += f.ui.line + 4;
            }
        }
    }
}

pub fn render(c: &mut Canvas, f: &Fonts, d: &Desk) {
    let t = d.theme;
    let l = layout(c, &t);
    c.vgradient(0, 0, c.w, c.h, t.wall.0, t.wall.1);
    // leggera vignettatura in basso
    for i in 0..60 {
        c.fill_a(0, c.h - 60 + i, c.w, 1, 0x000000, (i as u32) / 3);
    }

    // finestre: prima quelle non ingrandite, il terminale per ultimo (in primo piano)
    let maxed = d.wins.iter().find(|w| w.open && w.max).map(|w| w.kind);
    if let Some(k) = maxed {
        let r = win_rect(c, &l, k, true);
        draw_window(c, f, d, k, r);
    } else {
        for k in [WinKind::System, WinKind::Brain, WinKind::Terminal] {
            if d.wins.iter().any(|w| w.kind == k && w.open) {
                let r = win_rect(c, &l, k, false);
                draw_window(c, f, d, k, r);
            }
        }
    }

    // barra dei menu
    if t.menubar {
        c.fill_a(0, 0, c.w, l.top, if t.dark { 0x0E0F11 } else { 0xFFFFFF }, 200);
        let x = c.text(&f.bold, 14, 6, "B-AI-S", t.fg());
        let mut x = x + 18;
        for m in ["Sistema", "Brain", "Modelli", "Aiuto"] {
            x = c.text(&f.ui, x, 6, m, t.fg()) + 16;
        }
        let tw = f.ui.width(&d.tray);
        c.text(&f.ui, c.w - tw - 14, 6, &d.tray, t.fg());
    }

    // campo di richiesta
    let (px, py, pw, ph) = l.prompt;
    c.save_under(px - 20, py - 12, pw + 40, ph + 24);
    draw_prompt(c, f, d, px, py, pw, ph);

    // dock o barra delle applicazioni
    let apps = [WinKind::Terminal, WinKind::Brain, WinKind::System];
    if t.dock {
        let n = apps.len() as i32 + 1;
        let dw = n * 56 + 20;
        let dh = 64;
        let dx = (c.w - dw) / 2;
        let dy = c.h - dh - 8;
        if t.shadows {
            c.shadow(dx, dy, dw, dh, 18, 10, 90);
        }
        c.rrect(dx, dy, dw, dh, 18, if t.dark { 0x26282D } else { 0xFFFFFF }, 190);
        c.rrect_outline(dx, dy, dw, dh, 18, if t.dark { 0x45484F } else { 0xD8D8DD }, 200);
        for (i, k) in apps.iter().enumerate() {
            let ix = dx + 14 + i as i32 * 56;
            icon(c, f, &t, ix, dy + 9, 46, *k);
            if d.wins.iter().any(|w| w.kind == *k && w.open) {
                c.circle(ix + 23, dy + 60, 2, t.fg(), 255);
            }
        }
        // icona del cervello locale / modelli
        let ix = dx + 14 + apps.len() as i32 * 56;
        c.rrect(ix, dy + 9, 46, 46, 12, darken(t.accent, 60), 255);
        c.circle(ix + 23, dy + 32, 9, lighten(t.accent, 90), 255);
        c.circle(ix + 23, dy + 32, 4, darken(t.accent, 80), 255);
    } else {
        let bh = 48;
        let by = c.h - bh;
        c.fill_a(0, by, c.w, bh, t.bar_bg(), 235);
        c.fill(0, by, c.w, 1, if t.dark { 0x33363C } else { 0xD0D0D5 });
        let total = (apps.len() as i32 + 1) * 46;
        let mut ix = (c.w - total) / 2;
        // pulsante start
        c.rrect(ix + 6, by + 8, 32, 32, 6, t.accent, 255);
        for q in 0..4 {
            let qx = ix + 13 + (q % 2) * 10;
            let qy = by + 15 + (q / 2) * 10;
            c.fill(qx, qy, 8, 8, 0xFFFFFF);
        }
        ix += 46;
        for k in apps.iter() {
            icon(c, f, &t, ix + 6, by + 8, 32, *k);
            if d.wins.iter().any(|w| w.kind == *k && w.open) {
                c.rrect(ix + 16, by + 43, 12, 3, 1, t.accent, 255);
            }
            ix += 46;
        }
        let tw = f.ui.width(&d.tray);
        c.text(&f.ui, c.w - tw - 16, by + 15, &d.tray, t.fg());
    }
}

fn icon(c: &mut Canvas, f: &Fonts, t: &Theme, x: i32, y: i32, s: i32, k: WinKind) {
    let base = match k {
        WinKind::Terminal => 0x2B2F36,
        WinKind::Brain => mix(t.accent, 0x000000, 40),
        WinKind::System => 0x5E6673,
    };
    c.rrect(x, y, s, s, s / 4, base, 255);
    let lt = k.letter();
    let fnt = &f.bold;
    let w = fnt.width(lt);
    c.text(fnt, x + (s - w) / 2, y + (s - fnt.line) / 2, lt, 0xFFFFFF);
}

pub fn prompt_rect(c: &Canvas, d: &Desk) -> (i32, i32, i32, i32) {
    layout(c, &d.theme).prompt
}

pub fn draw_prompt(c: &mut Canvas, f: &Fonts, d: &Desk, px: i32, py: i32, pw: i32, ph: i32) {
    let t = &d.theme;
    // ripristina lo sfondo salvato sotto il campo
    c.restore_under();
    if t.shadows {
        c.shadow(px, py, pw, ph, ph / 2, 8, 80);
    }
    c.rrect(px, py, pw, ph, ph / 2, if t.dark { 0x23252A } else { 0xFFFFFF }, 245);
    c.rrect_outline(px, py, pw, ph, ph / 2, t.accent, 160);
    let tx = px + 20;
    let ty = py + (ph - f.ui.line) / 2;
    if d.input.is_empty() {
        c.text(&f.ui, tx, ty, "Chiedi a B-AI-S…  (aiuto per i comandi)", t.dim());
        c.fill(tx - 3, ty + 1, 2, f.ui.line - 2, t.accent);
    } else {
        // mostra la coda del testo se è troppo lungo
        let maxw = pw - 44;
        let mut s: &str = &d.input;
        while f.ui.width(s) > maxw {
            let mut it = s.chars();
            it.next();
            s = it.as_str();
        }
        let end = c.text(&f.ui, tx, ty, s, t.fg());
        c.fill(end + 1, ty + 1, 2, f.ui.line - 2, t.accent);
    }
}

/// Schermata di nascita: un'impronta generativa che dipende dal corpo (seriale, core, memoria).
pub fn splash(c: &mut Canvas, f: &Fonts, seed: u64, line1: &str, line2: &str) {
    let mut s = seed | 1;
    let mut rnd = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let hue = |v: u64| -> u32 {
        // tavolozza calda/fredda derivata dal seme
        let pal = [0xE0A43A, 0x4FB6A8, 0x9B6BFF, 0xE5674D, 0x5B8DEF, 0xC9D15A];
        pal[(v % pal.len() as u64) as usize]
    };
    c.fill(0, 0, c.w, c.h, 0x0B0C0E);
    let cx = c.w / 2;
    let cy = c.h / 2 - 30;
    let petals = 5 + (rnd() % 7) as i32;
    let c1 = hue(rnd());
    let c2 = hue(rnd());
    let base_r = (c.h / 5) as f32;
    for i in 0..petals {
        let ang = i as f32 * 6.2831853 / petals as f32 + (rnd() % 100) as f32 / 400.0;
        let dist = base_r * (0.35 + (rnd() % 30) as f32 / 100.0);
        let rr = (base_r * (0.28 + (rnd() % 25) as f32 / 100.0)) as i32;
        let px = cx + (libm::cosf(ang) * dist) as i32;
        let py = cy + (libm::sinf(ang) * dist) as i32;
        let col = if i % 2 == 0 { c1 } else { c2 };
        for k in (0..rr).step_by(3) {
            c.circle(px, py, rr - k, col, 18 + k as u32 / 4);
        }
    }
    c.circle(cx, cy, (base_r * 0.22) as i32, 0x0B0C0E, 255);
    c.circle(cx, cy, (base_r * 0.12) as i32, lighten(c1, 60), 255);
    let tw = f.large.width("B-AI-S");
    c.text(&f.large, cx - tw / 2, cy + base_r as i32 + 30, "B-AI-S", 0xF2F2F2);
    let w1 = f.ui.width(line1);
    c.text(&f.ui, cx - w1 / 2, cy + base_r as i32 + 78, line1, 0x9A9EA6);
    let w2 = f.ui.width(line2);
    c.text(&f.ui, cx - w2 / 2, cy + base_r as i32 + 100, line2, 0x6C7078);
}

/// schermata d'errore (eccezione della CPU o panic)
pub fn fatal(c: &mut Canvas, f: &Fonts, title: &str, lines: &[String]) {
    c.fill(0, 0, c.w, c.h, 0x3A0D12);
    c.text(&f.large, 40, 40, title, 0xFFFFFF);
    let mut y = 100;
    for l in lines {
        c.text(&f.mono, 40, y, l, 0xF3C9CC);
        y += f.mono.line + 2;
    }
}
