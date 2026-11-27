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

/// Applica al tema le etichette prodotte dal classificatore `bais-intent`.
/// Restituisce le modifiche, oppure `None` se le etichette non riguardano il tema
/// (`#altro`, nessuna etichetta o un'etichetta sconosciuta): allora decidono le parole chiave.
pub fn theme_tags(t: &mut Theme, tags: &str) -> Option<Vec<&'static str>> {
    let mut ch = Vec::new();
    for raw in tags.split_whitespace() {
        let tag = raw.strip_prefix('#')?;
        match tag {
            "macos" => { *t = Theme::MACOS; ch.push("stile macOS") }
            "windows" => { *t = Theme::WIN11; ch.push("stile Windows 11") }
            "mix" => { *t = Theme::MIX; ch.push("stile mix") }
            "bais" => { *t = Theme::BAIS; ch.push("stile B-AI-S") }
            "scuro" => { t.dark = true; ch.push("aspetto scuro") }
            "chiaro" => { t.dark = false; ch.push("aspetto chiaro") }
            "btn_dx" => { t.btn_right = true; ch.push("pulsanti a destra") }
            "btn_sx" => { t.btn_right = false; ch.push("pulsanti a sinistra") }
            "tit_centro" => { t.title_center = true; ch.push("titoli centrati") }
            "tit_sx" => { t.title_center = false; ch.push("titoli a sinistra") }
            "angoli_0" => { t.radius = 0; ch.push("angoli squadrati") }
            "angoli_11" => { t.radius = 11; ch.push("angoli arrotondati") }
            "angoli_18" => { t.radius = 18; ch.push("angoli molto arrotondati") }
            "menu_si" => { t.menubar = true; ch.push("barra dei menu") }
            "menu_no" => { t.menubar = false; ch.push("senza barra dei menu") }
            "ombre_si" => { t.shadows = true; ch.push("ombre") }
            "ombre_no" => { t.shadows = false; ch.push("senza ombre") }
            "taskbar" => { t.dock = false; ch.push("barra di Windows") }
            "dock" => { t.dock = true; ch.push("dock") }
            _ => {
                if let Some(c) = tag.strip_prefix("acc_") {
                    t.accent = COLORS.iter().find(|x| x.0 == c)?.1;
                    ch.push("colore d'accento");
                } else if let Some(c) = tag.strip_prefix("sfondo_") {
                    let c = COLORS.iter().find(|x| x.0 == c)?.1;
                    t.wall = (crate::gfx::lighten(c, 30), crate::gfx::darken(c, 140));
                    ch.push("sfondo");
                } else {
                    return None; // "altro" o sconosciuta
                }
            }
        }
    }
    if ch.is_empty() { None } else { Some(ch) }
}

const HELP: &str = "Stili: stile macos · stile windows 11 · mix · ripristina
Modifiche: scuro/chiaro · pulsanti a destra/sinistra · titoli centrati · angoli squadrati/arrotondati · accento viola · sfondo verde · con la barra di windows · con il dock · senza ombre · senza barra dei menu · annulla
Esempio: macos scuro con la barra di windows e accento viola
Finestre: apri/chiudi/ingrandisci terminale|brain|sistema · pulisci
Brain: ricorda <testo> · cerca <parole> · dov'è …? · cosa sai di … · dimentica tutto
Parlami anche liberamente: ciao · come stai? · chi sei? · una domanda su ciò che mi hai detto di ricordare
Sistema: hardware · temperatura · diario · abitudini · tastiera it|us · riavvia · spegni
Modelli: modelli · carica <n> · scarica <n> · predefinito <n> · genera <testo> · token <testo> · capisci <frase>";

/// Prime parole dei comandi, per suggerire il più vicino a un refuso.
const COMANDI: &[&str] = &[
    "aiuto", "stile", "ripristina", "ricorda", "cerca", "dimentica", "hardware", "temperatura",
    "diario", "abitudini", "tastiera", "riavvia", "spegni", "modelli", "carica", "scarica",
    "predefinito", "genera", "token", "capisci", "apri", "chiudi", "ingrandisci", "riduci",
    "pulisci", "annulla",
];

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

    /// Cerca nel brain le parole (≥ 3 lettere); true se ha trovato qualcosa.
    fn brain_search(&mut self, q: &str) -> bool {
        let qn = norm(q);
        let words: Vec<&str> = qn.split_whitespace().filter(|w| w.len() > 2).collect();
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
        if hits.is_empty() {
            return false;
        }
        hits.sort_by(|a, b| b.0.cmp(&a.0));
        let s: Vec<String> = hits.iter().take(5).map(|h| format!("[{}] {}", h.0, h.1)).collect();
        let s = s.join("\n");
        self.out(&s);
        true
    }

    /// Risposta a ciò che ha capito `nlu`; false se la frase resta non capita.
    fn nlu_reply(&mut self, i: crate::nlu::Intento) -> bool {
        use crate::nlu::Intento;
        match i {
            Intento::Ricorda(t) => {
                self.desk.notes.push(t);
                self.out(&format!("Ricordato. Il brain ha {} ricordi (in RAM: spariscono allo spegnimento).", self.desk.notes.len()));
            }
            Intento::Cerca(k) => {
                if !self.brain_search(&k) {
                    self.out("Nel brain non c'è niente di pertinente.");
                }
            }
            Intento::Domanda(k) => {
                if !self.brain_search(&k) {
                    self.out("Non lo so ancora. Se vuoi che lo sappia, dimmelo con «ricorda …».");
                }
            }
            Intento::ComeStai => {
                let (used, total) = crate::heap::stats();
                let sec = crate::plat::millis() / 1000;
                let s = format!(
                    "Sto bene. Sono acceso da {} h {:02} min, uso {} MB di heap su {} MB e ho {} ricordi nel brain.",
                    sec / 3600,
                    (sec / 60) % 60,
                    used >> 20,
                    total >> 20,
                    self.desk.notes.len()
                );
                self.out(&s);
            }
            Intento::ChiSei => {
                let s = format!(
                    "Sono B-AI-S, kernel-ai {} su {}. Impronta del corpo {:016x}. Scrivi «aiuto» per vedere cosa so fare.",
                    crate::VERSION,
                    hw::machine_name(),
                    hw::fingerprint()
                );
                self.out(&s);
            }
            Intento::Saluto => self.out("Ciao Bais. Scrivi «aiuto» se ti servono i comandi."),
            Intento::Grazie => self.out("Di niente."),
            Intento::Sconosciuto { suggerimento: Some(c) } => {
                self.err(&format!("Non ho capito. Forse intendevi «{}»? Scrivi «aiuto».", c));
                return false;
            }
            Intento::Sconosciuto { suggerimento: None } => {
                self.err("Non ho capito: non è un comando che conosco e non riguarda il tema. Scrivi «aiuto».");
                return false;
            }
        }
        true
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
        let first0 = p.split_whitespace().next().unwrap_or("");
        let first = match first0.trim_end_matches(|c: char| matches!(c, '.' | '!' | ',' | '?' | ':')) {
            "" => first0,
            t => t,
        };
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
            "tastiera" | "keyboard" => {
                let l = p.split_whitespace().nth(1).unwrap_or("");
                match l {
                    "us" | "usa" | "inglese" => crate::plat::set_layout(crate::plat::Layout::Us),
                    "it" | "ita" | "italiana" | "italiano" => crate::plat::set_layout(crate::plat::Layout::It),
                    _ => {}
                }
                let cur = if crate::plat::layout() == crate::plat::Layout::It { "italiana" } else { "americana (US)" };
                self.out(&format!("Tastiera {}. Cambia con «tastiera it» o «tastiera us».", cur));
                return true;
            }
            "riavvia" | "reboot" => {
                self.note("Riavvio...");
                redraw(self);
                crate::plat::delay_ms(300);
                crate::plat::reboot();
            }
            "spegni" | "spegniti" | "poweroff" | "shutdown" => {
                self.note("Spegnimento...");
                redraw(self);
                crate::plat::delay_ms(300);
                crate::plat::poweroff();
                crate::plat::delay_ms(1500); // l'emulatore si chiude qui; se no, ripiego
                self.note("Questa macchina non si spegne da sola: ora puoi spegnerla.");
                redraw(self);
                crate::plat::halt();
            }
            "temperatura" => {
                match crate::plat::temperature() {
                    Some(t) => self.out(&format!("Temperatura della CPU: {:.1} °C", t as f32 / 1000.0)),
                    None => self.err("Non leggo ancora i sensori di temperatura di questo PC (MSR/ACPI da scrivere)."),
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
            let q = String::from(p.trim_start_matches("cerca").trim().trim_start_matches("nel brain").trim());
            if !self.brain_search(&q) {
                self.out("Nel brain non c'è niente di pertinente.");
            }
            return true;
        }
        // comandi del gestore modelli (crate bais-model)
        if first == "capisci" {
            // mostra cosa capisce il classificatore, senza applicarlo
            let q = p.trim_start_matches("capisci").trim();
            let t0 = crate::plat::millis();
            match self.models.classify(q, 10) {
                None => self.err("Nessun classificatore degli intenti caricato."),
                Some((_, tags)) => {
                    let ms = crate::plat::millis() - t0;
                    self.out(&format!("intento:{}  ({} ms)", tags, ms));
                }
            }
            return true;
        }
        if matches!(first, "modelli" | "carica" | "scarica" | "predefinito" | "genera" | "token") {
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
        // frasi libere (ricorda/cerca in molte forme, cortesia) prima del classificatore,
        // che con le frasi fuori tema sbaglia
        if let Some(i) = crate::nlu::capisci_prima(raw) {
            return self.nlu_reply(i);
        }
        // tema, prima strada: lo scheduler sceglie il classificatore degli intenti
        let t0 = crate::plat::millis();
        if let Some((id, tags)) = self.models.classify(p, 10) {
            let ms = crate::plat::millis() - t0;
            crate::log!("[scheduler] classificatore «{}»:{} ({} ms)\n", self.models.name(id), tags, ms);
            let mut t = self.desk.theme;
            if let Some(ch) = theme_tags(&mut t, &tags) {
                self.history.push(self.desk.theme);
                if self.history.len() > 32 {
                    self.history.remove(0);
                }
                self.desk.theme = t;
                let s = format!("Fatto: {}.  (capito da {}, {} ms)", ch.join(", "), self.models.name(id), ms);
                self.out(&s);
                return true;
            }
        }
        // tema, ripiego: parole chiave della v0.2
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
        // ripiego: domanda al brain, oppure suggerimento del comando più vicino
        let i = crate::nlu::capisci_dopo(raw, COMANDI);
        self.nlu_reply(i)
    }
}
