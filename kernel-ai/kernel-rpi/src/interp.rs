//! Interprete delle frasi: tema da frase (stili, modifiche combinate, annulla), finestre,
//! brain in RAM, diario e abitudini, hardware, comandi del gestore modelli.
//! Oggi è a parole chiave, come la v0.2; il gestore modelli è già agganciato.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::hw;
use crate::ui::{Desk, LineKind, Theme, WinKind};
use bais_model::manager::Manager;

pub struct Sys {
    pub desk: Desk,
    pub models: Manager<'static>,
    pub history: Vec<Theme>,
    pub diary: Vec<String>,
    /// coppie (A, B) già proposte; `true` se Bais ha detto sì
    pub habits: Vec<(String, String, bool)>,
    /// abitudine in attesa di risposta
    pub pending: Option<(String, String)>,
    pub last_tps: Option<f32>,
}

const COLORS: &[(&str, u32)] = &[
    ("viola", 0x9B6BFF),
    ("blu", 0x0A84FF),
    ("azzurro", 0x4FB3F6),
    ("verde", 0x30B85C),
    ("turchese", 0x2EC4B6),
    ("rosso", 0xE5484D),
    ("arancione", 0xF0883E),
    ("arancio", 0xF0883E),
    ("giallo", 0xE8C547),
    ("ambra", 0xE0A43A),
    ("rosa", 0xE86BA8),
    ("grigio", 0x8D9199),
    ("nero", 0x15161A),
    ("bianco", 0xF2F2F5),
];

fn color_after(s: &str, key: &str) -> Option<u32> {
    let i = s.find(key)? + key.len();
    let rest = s[i..].trim_start();
    let rest = rest.strip_prefix(':').unwrap_or(rest).trim_start();
    COLORS.iter().find(|(n, _)| rest.starts_with(n)).map(|c| c.1)
}

fn norm(s: &str) -> String {
    let mut o = String::new();
    for ch in s.trim().chars() {
        let c = match ch {
            'À' | 'à' | 'á' => 'a',
            'È' | 'É' | 'è' | 'é' => 'e',
            'Ì' | 'ì' | 'í' => 'i',
            'Ò' | 'ò' | 'ó' => 'o',
            'Ù' | 'ù' | 'ú' => 'u',
            c => c.to_ascii_lowercase(),
        };
        o.push(c);
    }
    o
}

/// Applica al tema una frase; restituisce le modifiche riconosciute.
pub fn theme_phrase(t: &mut Theme, p: &str) -> Vec<&'static str> {
    let mut ch = Vec::new();
    // «con la barra di windows» è un dettaglio, non lo stile intero
    let mut s = String::from(p);
    let mut taskbar = false;
    for k in ["la barra di windows", "barra di windows", "barra windows", "taskbar", "barra delle applicazioni"] {
        if let Some(i) = s.find(k) {
            s.replace_range(i..i + k.len(), " ");
            taskbar = true;
        }
    }
    let mut dock = false;
    for k in ["il dock di macos", "dock di macos", "il dock", "dock"] {
        if let Some(i) = s.find(k) {
            s.replace_range(i..i + k.len(), " ");
            dock = true;
        }
    }
    if s.contains("ripristina") || s.contains("predefinito") || s.contains("stile b-ai-s") {
        *t = Theme::BAIS;
        ch.push("stile B-AI-S");
    }
    if s.contains("macos") || s.contains("stile mac") || s.contains("apple") {
        *t = Theme::MACOS;
        ch.push("stile macOS");
    }
    if s.contains("windows") || s.contains("win11") {
        *t = Theme::WIN11;
        ch.push("stile Windows 11");
    }
    if s.split_whitespace().any(|w| w == "mix") {
        *t = Theme::MIX;
        ch.push("stile mix");
    }
    if s.contains("scuro") || s.contains("dark") || s.contains("notte") {
        t.dark = true;
        ch.push("aspetto scuro");
    }
    if s.contains("chiaro") || s.contains("light") || s.contains("giorno") {
        t.dark = false;
        ch.push("aspetto chiaro");
    }
    if s.contains("pulsanti a destra") || s.contains("bottoni a destra") {
        t.btn_right = true;
        ch.push("pulsanti a destra");
    }
    if s.contains("pulsanti a sinistra") || s.contains("bottoni a sinistra") {
        t.btn_right = false;
        ch.push("pulsanti a sinistra");
    }
    if s.contains("titoli centrati") || s.contains("titolo centrato") || s.contains("titoli al centro") {
        t.title_center = true;
        ch.push("titoli centrati");
    }
    if s.contains("titoli a sinistra") || s.contains("titolo a sinistra") {
        t.title_center = false;
        ch.push("titoli a sinistra");
    }
    if s.contains("angoli squadrati") || s.contains("angoli retti") || s.contains("senza angoli") {
        t.radius = 0;
        ch.push("angoli squadrati");
    } else if s.contains("molto arrotondat") {
        t.radius = 18;
        ch.push("angoli molto arrotondati");
    } else if s.contains("arrotondat") {
        t.radius = 11;
        ch.push("angoli arrotondati");
    }
    if let Some(c) = color_after(&s, "accento").or_else(|| color_after(&s, "colore")) {
        t.accent = c;
        ch.push("colore d'accento");
    }
    if let Some(c) = color_after(&s, "sfondo") {
        t.wall = (crate::gfx::lighten(c, 30), crate::gfx::darken(c, 140));
        ch.push("sfondo");
    }
    if s.contains("senza barra dei menu") || s.contains("togli la barra dei menu") || s.contains("nascondi la barra dei menu") {
        t.menubar = false;
        ch.push("senza barra dei menu");
    } else if s.contains("barra dei menu") {
        t.menubar = true;
        ch.push("barra dei menu");
    }
    if s.contains("senza ombre") || s.contains("togli le ombre") {
        t.shadows = false;
        ch.push("senza ombre");
    } else if s.contains("ombre") {
        t.shadows = true;
        ch.push("ombre");
    }
    if taskbar {
        t.dock = false;
        ch.push("barra di Windows");
    }
    if dock {
        t.dock = true;
        ch.push("dock");
    }
    ch
}

const HELP: &str = "Stili: stile macos · stile windows 11 · mix · ripristina
Modifiche: scuro/chiaro · pulsanti a destra/sinistra · titoli centrati · angoli squadrati/arrotondati · accento viola · sfondo verde · con la barra di windows · con il dock · senza ombre · senza barra dei menu · annulla
Esempio: macos scuro con la barra di windows e accento viola
Finestre: apri/chiudi/ingrandisci terminale|brain|sistema · pulisci
Brain: ricorda: <testo> · cerca <parole> · dimentica tutto
Sistema: hardware · temperatura · diario · abitudini
Modelli: modelli · carica <n> · scarica <n> · predefinito <n> · genera <testo>";

impl Sys {
    fn out(&mut self, s: &str) {
        crate::log!("{}\n", s);
        self.desk.say(s, LineKind::Out);
    }
    fn note(&mut self, s: &str) {
        crate::log!("{}\n", s);
        self.desk.say(s, LineKind::Note);
    }
    fn err(&mut self, s: &str) {
        crate::log!("{}\n", s);
        self.desk.say(s, LineKind::Err);
    }

    /// Esegue una frase scritta da Bais. `redraw` viene chiamata durante la generazione.
    pub fn handle(&mut self, raw: &str, redraw: &mut dyn FnMut(&mut Sys)) {
        let raw = raw.trim();
        if raw.is_empty() {
            return;
        }
        self.desk.say(raw, LineKind::Input);
        let p = norm(raw);

        // risposta a una proposta di abitudine
        if let Some((a, b)) = self.pending.take() {
            if matches!(p.as_str(), "si" | "sì" | "ok" | "va bene" | "certo" | "si grazie") {
                if let Some(h) = self.habits.iter_mut().find(|h| h.0 == a && h.1 == b) {
                    h.2 = true;
                }
                self.out(&format!("D'accordo: d'ora in poi dopo «{}» eseguo da solo «{}».", a, b));
                return;
            }
            if matches!(p.as_str(), "no" | "no grazie" | "meglio di no") {
                self.out("Va bene, non lo farò da solo. Non te lo chiederò più.");
                return;
            }
        }

        let ok = self.execute(raw, &p, redraw);
        if !ok {
            return;
        }
        // diario e abitudini
        let prev = self.diary.last().cloned();
        self.diary.push(p.clone());
        if let Some(a) = prev {
            if a != p {
                let n = self.diary.windows(2).filter(|w| w[0] == a && w[1] == p).count();
                let known = self.habits.iter().any(|h| h.0 == a && h.1 == p);
                if n >= 2 && !known {
                    self.habits.push((a.clone(), p.clone(), false));
                    self.pending = Some((a.clone(), p.clone()));
                    self.note(&format!("Ho notato che dopo «{}» chiedi spesso «{}». Posso farlo da solo d'ora in poi? (sì/no)", a, p));
                }
            }
        }
        // abitudine accettata: esegue la frase successiva
        let auto: Option<String> = self.habits.iter().find(|h| h.2 && h.0 == p).map(|h| h.1.clone());
        if let Some(b) = auto {
            self.note(&format!("abitudine: eseguo «{}»", b));
            let bn = norm(&b);
            if self.execute(&b, &bn, redraw) {
                self.diary.push(bn);
            }
        }
    }

    /// true se la frase è stata capita
    fn execute(&mut self, raw: &str, p: &str, redraw: &mut dyn FnMut(&mut Sys)) -> bool {
        let first = p.split_whitespace().next().unwrap_or("");
        match first {
            "aiuto" | "help" | "?" | "comandi" => {
                self.out(HELP);
                return true;
            }
            "pulisci" | "clear" => {
                self.desk.term.clear();
                return true;
            }
            "hardware" | "corpo" => {
                let r = hw::report(self);
                self.out(&r);
                return true;
            }
            "temperatura" => {
                match crate::plat::temperature() {
                    Some(t) => self.out(&format!("Temperatura del SoC: {:.1} °C", t as f32 / 1000.0)),
                    None => self.err("Il firmware non fornisce la temperatura."),
                }
                return true;
            }
            "diario" => {
                if self.diary.is_empty() {
                    self.out("Il diario è vuoto.");
                } else {
                    let start = self.diary.len().saturating_sub(15);
                    let lines: Vec<String> = self.diary[start..].iter().enumerate().map(|(i, s)| format!("{:>3}. {}", start + i + 1, s)).collect();
                    let s = lines.join("\n");
                    self.out(&s);
                }
                return true;
            }
            "abitudini" => {
                if self.habits.is_empty() {
                    self.out("Nessuna abitudine riconosciuta. Ripeti due volte la stessa coppia di richieste.");
                } else {
                    let s: Vec<String> = self
                        .habits
                        .iter()
                        .map(|h| format!("«{}» → «{}»  {}", h.0, h.1, if h.2 { "automatica" } else { "rifiutata o in attesa" }))
                        .collect();
                    let s = s.join("\n");
                    self.out(&s);
                }
                return true;
            }
            "annulla" => {
                match self.history.pop() {
                    Some(t) => {
                        self.desk.theme = t;
                        self.out("Tema precedente ripristinato.");
                    }
                    None => self.err("Niente da annullare."),
                }
                return true;
            }
            "apri" | "chiudi" | "ingrandisci" | "riduci" => {
                let k = if p.contains("terminal") {
                    Some(WinKind::Terminal)
                } else if p.contains("brain") || p.contains("cervello") {
                    Some(WinKind::Brain)
                } else if p.contains("sistema") {
                    Some(WinKind::System)
                } else {
                    None
                };
                match k {
                    None => self.err("Quale finestra? terminale, brain o sistema."),
                    Some(k) => {
                        for w in self.desk.wins.iter_mut() {
                            if first == "ingrandisci" {
                                w.max = w.kind == k;
                            } else if first == "riduci" {
                                w.max = false;
                            }
                        }
                        let w = self.desk.win(k);
                        match first {
                            "apri" => w.open = true,
                            "chiudi" => {
                                w.open = false;
                                w.max = false;
                            }
                            _ => w.open = true,
                        }
                        self.note(&format!("{}: {}", k.title(), first));
                    }
                }
                return true;
            }
            _ => {}
        }
        if let Some(rest) = raw.strip_prefix("ricorda:").or_else(|| raw.strip_prefix("Ricorda:")) {
            let t = rest.trim();
            if t.is_empty() {
                self.err("Cosa devo ricordare? ricorda: <testo>");
            } else {
                self.desk.notes.push(t.to_string());
                self.out(&format!("Ricordato. Il brain ha {} ricordi (in RAM: spariscono allo spegnimento).", self.desk.notes.len()));
            }
            return true;
        }
        if first == "dimentica" && p.contains("tutto") {
            self.desk.notes.clear();
            self.out("Brain svuotato.");
            return true;
        }
        if first == "cerca" {
            let q = p.trim_start_matches("cerca").trim().trim_start_matches("nel brain").trim();
            let words: Vec<&str> = q.split_whitespace().filter(|w| w.len() > 2).collect();
            let mut hits: Vec<(usize, &String)> = self
                .desk
                .notes
                .iter()
                .map(|n| {
                    let nn = norm(n);
                    (words.iter().filter(|w| nn.contains(*w)).count(), n)
                })
                .filter(|h| h.0 > 0)
                .collect();
            hits.sort_by(|a, b| b.0.cmp(&a.0));
            if hits.is_empty() {
                self.out("Nel brain non c'è niente di pertinente.");
            } else {
                let s: Vec<String> = hits.iter().take(5).map(|h| format!("[{}] {}", h.0, h.1)).collect();
                let s = s.join("\n");
                self.out(&s);
            }
            return true;
        }
        // comandi del gestore modelli (crate bais-model)
        if matches!(first, "modelli" | "carica" | "scarica" | "predefinito" | "genera") {
            let t0 = crate::plat::millis();
            let mut count = 0usize;
            let line = String::from(raw);
            // la generazione scrive i token man mano nel terminale
            let mut pieces: Vec<String> = Vec::new();
            let r = {
                let models = &mut self.models;
                bais_model::shell::run(models, &line, 64, |piece| {
                    count += 1;
                    crate::log!("{}", piece);
                    pieces.push(String::from(piece));
                })
            };
            if first == "genera" && !pieces.is_empty() {
                self.desk.say("", LineKind::Out);
                for pc in &pieces {
                    self.desk.append(pc);
                }
                redraw(self);
                let dt = (crate::plat::millis() - t0).max(1) as f32 / 1000.0;
                self.last_tps = Some(count as f32 / dt);
            }
            if let Some(s) = r {
                let s = s.trim_start_matches('\n').to_string();
                if first == "genera" {
                    let tps = self.last_tps.unwrap_or(0.0);
                    self.note(&format!("{}  {:.1} token/s su un core", s, tps));
                } else {
                    self.out(&s);
                }
            }
            return true;
        }
        // tema
        let mut t = self.desk.theme;
        let ch = theme_phrase(&mut t, p);
        if !ch.is_empty() {
            self.history.push(self.desk.theme);
            if self.history.len() > 32 {
                self.history.remove(0);
            }
            self.desk.theme = t;
            let s = format!("Fatto: {}.", ch.join(", "));
            self.out(&s);
            return true;
        }
        self.err("Non ho capito. Per ora capisco solo le formulazioni previste: scrivi «aiuto».");
        false
    }
}
