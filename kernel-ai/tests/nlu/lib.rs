//! nlu — comprensione tollerante delle frasi per l'interprete di kernel-ai.
//!
//! Si mette tra il confronto esatto dei comandi e il classificatore
//! `bais-intent`: riconosce «ricorda» e «cerca» in molte forme, la cortesia
//! («ciao», «come stai?») e le domande, e al posto di «Non ho capito»
//! propone il comando più vicino quando la frase sembra un refuso.
//!
//! `no_std` + `alloc`, nessuna dipendenza. Si copia in `kernel-pc/src/nlu.rs`
//! (togliendo le due righe `#![no_std]` / `extern crate alloc` se il crate
//! del kernel le dichiara già).
//!
//! Ordine consigliato nell'interprete:
//!   1. comando esatto (come oggi), provato anche su `normalizza(riga)`;
//!   2. `capisci_prima(riga)`  → Ricorda / Cerca / cortesia;
//!   3. classificatore `bais-intent` (solo se 2 non ha riconosciuto nulla);
//!   4. se il classificatore dice `#altro`: `capisci_dopo(riga, COMANDI)`
//!      → Domanda (va al brain) oppure Sconosciuto con suggerimento.
//! Così «ricorda il server è emulato» non arriva mai al classificatore del
//! tema, che con le frasi fuori tema sbaglia.

#![cfg_attr(not(test), no_std)]
extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Cosa ha capito il parser.
#[derive(Debug, PartialEq, Eq)]
pub enum Intento {
    /// Testo da salvare nel brain, con le maiuscole originali.
    Ricorda(String),
    /// Parole da cercare nel brain (articoli e preposizioni tolti).
    Cerca(String),
    Saluto,
    ComeStai,
    ChiSei,
    Grazie,
    /// Domanda libera: parole chiave per la ricerca nel brain.
    Domanda(String),
    /// Niente di riconoscibile; eventuale comando simile a quello scritto.
    Sconosciuto { suggerimento: Option<&'static str> },
}

// ---------------------------------------------------------------------------
// Normalizzazione
// ---------------------------------------------------------------------------

/// Minuscolo, apostrofi uniformati, «e'» → «è», punteggiatura finale tolta,
/// spazi compattati. Da usare anche prima del confronto esatto dei comandi
/// («Aiuto.» → «aiuto»).
pub fn normalizza(riga: &str) -> String {
    let mut s = String::with_capacity(riga.len());
    let mut spazio = true; // evita spazi iniziali e doppi
    for c in riga.chars() {
        let c = match c {
            '\u{2018}' | '\u{2019}' | '`' | '\u{b4}' => '\'',
            '\t' | '\r' | '\n' => ' ',
            _ => c,
        };
        if c == ' ' {
            if !spazio {
                s.push(' ');
                spazio = true;
            }
            continue;
        }
        spazio = false;
        for l in c.to_lowercase() {
            s.push(l);
        }
    }
    // punteggiatura e spazi finali
    while let Some(c) = s.chars().last() {
        if matches!(c, ' ' | '?' | '!' | '.' | ',' | ';' | ':' | '…') {
            s.pop();
        } else {
            break;
        }
    }
    accenti_apostrofo(&s)
}

/// «e'» isolato → «è», «perche'» → «perché», «piu'» → «più», ecc.
fn accenti_apostrofo(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, w) in s.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let w2: &str = match w {
            "e'" => "è",
            "perche'" => "perché",
            "poiche'" => "poiché",
            "piu'" => "più",
            "puo'" => "può",
            "gia'" => "già",
            "cioe'" => "cioè",
            "cosi'" => "così",
            "c'e'" => "c'è",
            "dov'e'" => "dov'è",
            "com'e'" => "com'è",
            "cos'e'" => "cos'è",
            "qual'e'" | "qual'è" => "qual è",
            _ => w,
        };
        out.push_str(w2);
    }
    out
}

// ---------------------------------------------------------------------------
// Prefissi
// ---------------------------------------------------------------------------

const RICORDA: &[&str] = &[
    "ricorda:", "ricordati che ", "ricorda che ", "ricordati di ", "ricordati ",
    "ricorda ", "ricordami che ", "segnati che ", "segnati ", "segna che ",
    "annota che ", "annota:", "annota ", "prendi nota che ", "prendi nota:",
    "prendi nota ", "memorizza che ", "memorizza:", "memorizza ", "nota:",
    "salva nel brain ", "tieni a mente che ",
];

const CERCA: &[&str] = &[
    "cerca nel brain ", "cerca:", "cerca ", "trova ", "cosa sai di ", "cosa sai del ",
    "cosa sai della ", "cosa sai dello ", "cosa sai dei ", "cosa sai degli ",
    "cosa sai delle ", "cosa sai sul ", "cosa sai sulla ", "cosa sai su ",
    "che sai di ", "cosa ricordi di ", "cosa ricordi del ", "cosa ricordi della ",
    "cosa ricordi su ", "ti ricordi di ", "ti ricordi del ", "ti ricordi della ",
    "dov'è ", "dove è ", "dove si trova ", "dove sta ", "dove sono ",
    "dove si trovano ", "dove ho messo ",
];

/// Cortesia in testa alla frase, tolta prima di tutto il resto.
const CORTESIA: &[&str] = &[
    "per favore ", "per piacere ", "b-ai-s ", "b-ai-s, ", "bais ", "bais, ",
    "ehi ", "senti ", "scusa ", "puoi ", "potresti ",
];

const SALUTI: &[&str] = &[
    "ciao", "buongiorno", "buonasera", "buonanotte", "salve", "ehi", "hey",
    "hello", "ciao b-ai-s", "ciao bais", "eccomi", "sono tornato",
];
const COME_STAI: &[&str] = &[
    "come stai", "come va", "tutto bene", "tutto ok", "come ti senti", "stai bene",
    "come procede", "come andiamo",
];
const CHI_SEI: &[&str] = &[
    "chi sei", "come ti chiami", "cosa sei", "che cosa sei", "presentati",
    "chi sei tu", "cosa sai fare", "che sai fare",
];
const GRAZIE: &[&str] = &[
    "grazie", "grazie mille", "ti ringrazio", "perfetto grazie", "ok grazie",
    "bravo", "ottimo",
];

const INTERROGATIVI: &[&str] = &[
    "chi", "cosa", "che", "come", "dove", "dov'è", "quando", "perché", "quale",
    "quali", "quanto", "quanti", "quanta", "quante", "cos'è", "com'è", "c'è",
];

/// Parole vuote tolte dalle ricerche nel brain.
const VUOTE: &[&str] = &[
    "il", "lo", "la", "i", "gli", "le", "l'", "un", "uno", "una", "un'", "di", "del",
    "dello", "della", "dei", "degli", "delle", "a", "al", "allo", "alla", "ai",
    "agli", "alle", "da", "dal", "dalla", "dai", "in", "nel", "nello", "nella",
    "nei", "negli", "nelle", "su", "sul", "sulla", "sui", "con", "per", "tra",
    "fra", "e", "ed", "o", "che", "chi", "cosa", "come", "dove", "quando",
    "perché", "quale", "quali", "è", "sono", "sai", "mi", "ti", "ci", "si", "ho",
    "hai", "ha", "c'è", "dov'è", "cos'è", "com'è", "me", "te", "non", "qual",
    "messo", "trova", "trovano", "sta", "stanno", "ricordi",
];

/// Prefisso senza distinzione di maiuscole, confrontato sulla riga originale
/// (spazi iniziali già tolti). Restituisce il resto con le maiuscole originali.
fn togli_prefisso_ci<'a>(orig: &'a str, prefisso: &str) -> Option<&'a str> {
    let mut it = orig.char_indices();
    for p in prefisso.chars() {
        let (_, c) = it.next()?;
        let c = match c {
            '\u{2019}' | '\u{2018}' | '`' => '\'',
            _ => c,
        };
        if !c.to_lowercase().eq(p.to_lowercase()) {
            return None;
        }
    }
    let resto = match it.next() {
        Some((i, _)) => &orig[i..],
        None => "",
    };
    Some(resto)
}

fn togli_cortesia(mut s: &str) -> &str {
    loop {
        let t = s.trim_start();
        let mut cambiato = false;
        for p in CORTESIA {
            if let Some(r) = togli_prefisso_ci(t, p) {
                s = r;
                cambiato = true;
                break;
            }
        }
        if !cambiato {
            return t;
        }
    }
}

/// Toglie punteggiatura finale e spazi, senza toccare le maiuscole.
fn pulisci_coda(s: &str) -> &str {
    s.trim()
        .trim_end_matches(|c: char| matches!(c, '?' | '!' | '.' | ',' | ';' | ':' | ' ' | '…'))
        .trim()
}

/// Parole chiave per il brain: normalizzate, senza parole vuote né parole
/// sotto le 3 lettere (il brain le ignora comunque).
pub fn parole_chiave(s: &str) -> String {
    let n = normalizza(s);
    let mut out = String::new();
    for w in n.split(' ') {
        // «l'acqua» → «acqua»
        let w = match w.find('\'') {
            Some(i) if i + 1 < w.len() && !VUOTE.contains(&w) => &w[i + 1..],
            _ => w,
        };
        let w = w.trim_matches(|c: char| !c.is_alphanumeric());
        if w.chars().count() < 3 || VUOTE.contains(&w) {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    out
}

fn uguale_a_uno(n: &str, lista: &[&str]) -> bool {
    lista.iter().any(|x| *x == n)
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

/// Passo 2: da chiamare prima del classificatore. `None` = la frase va avanti
/// (classificatore del tema e poi `capisci_dopo`).
pub fn capisci_prima(riga: &str) -> Option<Intento> {
    let corpo = togli_cortesia(riga);
    let n = normalizza(corpo);
    if n.is_empty() {
        return None;
    }

    // cortesia a frase intera (anche «ciao, come stai?»)
    let senza_saluto = SALUTI
        .iter()
        .filter_map(|s| n.strip_prefix(s))
        .map(|r| r.trim_start_matches(|c: char| c == ',' || c == ' '))
        .find(|r| !r.is_empty());
    let n2 = senza_saluto.unwrap_or(&n);
    if uguale_a_uno(n2, COME_STAI) {
        return Some(Intento::ComeStai);
    }
    if uguale_a_uno(n2, CHI_SEI) {
        return Some(Intento::ChiSei);
    }
    if uguale_a_uno(&n, SALUTI) {
        return Some(Intento::Saluto);
    }
    if uguale_a_uno(&n, GRAZIE) {
        return Some(Intento::Grazie);
    }

    for p in RICORDA {
        if let Some(r) = togli_prefisso_ci(corpo, p) {
            let t = pulisci_coda(r);
            if !t.is_empty() {
                return Some(Intento::Ricorda(String::from(t)));
            }
        }
    }
    for p in CERCA {
        if let Some(r) = togli_prefisso_ci(corpo, p) {
            let k = parole_chiave(r);
            if !k.is_empty() {
                return Some(Intento::Cerca(k));
            }
        }
    }
    None
}

/// Passo 4: il classificatore ha detto `#altro`. `comandi` è l'elenco delle
/// prime parole dei comandi conosciuti, usato per suggerire un refuso.
pub fn capisci_dopo(riga: &str, comandi: &[&'static str]) -> Intento {
    let corpo = togli_cortesia(riga);
    let n = normalizza(corpo);
    let prima = n.split(' ').next().unwrap_or("");

    let interrogativa = riga.trim_end().ends_with('?') || INTERROGATIVI.contains(&prima);
    if interrogativa {
        let k = parole_chiave(&n);
        if !k.is_empty() {
            return Intento::Domanda(k);
        }
    }
    Intento::Sconosciuto { suggerimento: piu_vicino(prima, comandi) }
}

/// Comando con distanza di modifica ≤ 2 (≤ 1 per parole corte).
pub fn piu_vicino(parola: &str, comandi: &[&'static str]) -> Option<&'static str> {
    let n = parola.chars().count();
    if n < 3 {
        return None;
    }
    let soglia = if n <= 4 { 1 } else { 2 };
    let mut migliore: Option<(&'static str, usize)> = None;
    for c in comandi {
        let d = distanza(parola, c);
        if d == 0 {
            return None; // la parola è già un comando: il problema è altrove
        }
        if d <= soglia && migliore.map_or(true, |(_, m)| d < m) {
            migliore = Some((c, d));
        }
    }
    migliore.map(|(c, _)| c)
}

/// Levenshtein sui caratteri, una sola riga di memoria.
fn distanza(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut riga: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prec = riga[0];
        riga[0] = i + 1;
        for j in 0..b.len() {
            let tmp = riga[j + 1];
            let costo = if ca == b[j] { 0 } else { 1 };
            riga[j + 1] = (riga[j + 1] + 1).min(riga[j] + 1).min(prec + costo);
            prec = tmp;
        }
    }
    riga[b.len()]
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;

    const CMD: &[&str] = &[
        "aiuto", "stile", "ricorda", "cerca", "dimentica", "hardware", "temperatura",
        "diario", "abitudini", "tastiera", "riavvia", "spegni", "modelli", "carica",
        "scarica", "predefinito", "genera", "token", "capisci", "apri", "chiudi",
        "ingrandisci", "pulisci", "annulla",
    ];

    fn r(s: &str) -> Option<Intento> {
        Some(Intento::Ricorda(String::from(s)))
    }
    fn c(s: &str) -> Option<Intento> {
        Some(Intento::Cerca(String::from(s)))
    }

    #[test]
    fn frasi_dello_screenshot() {
        assert_eq!(capisci_prima("ricorda il server e' emulato"), r("il server e' emulato"));
        assert_eq!(capisci_prima("come stai?"), Some(Intento::ComeStai));
    }

    #[test]
    fn ricorda_varianti() {
        assert_eq!(capisci_prima("ricorda: il server è in cantina"), r("il server è in cantina"));
        assert_eq!(capisci_prima("Ricorda che il NAS è in soffitta."), r("il NAS è in soffitta"));
        assert_eq!(capisci_prima("ricordati che domani c'è la riunione"), r("domani c'è la riunione"));
        assert_eq!(capisci_prima("per favore segnati che Mario arriva alle 9"), r("Mario arriva alle 9"));
        assert_eq!(capisci_prima("prendi nota: chiave nel cassetto"), r("chiave nel cassetto"));
        assert_eq!(capisci_prima("RICORDA il backup"), r("il backup"));
        assert_eq!(capisci_prima("ricorda"), None);
        assert_eq!(capisci_prima("ricorda:   "), None);
    }

    #[test]
    fn cerca_varianti() {
        assert_eq!(capisci_prima("cerca server"), c("server"));
        assert_eq!(capisci_prima("dov'è il server?"), c("server"));
        assert_eq!(capisci_prima("dov’è il server?"), c("server"));
        assert_eq!(capisci_prima("dove si trova la chiave"), c("chiave"));
        assert_eq!(capisci_prima("cosa sai del server emulato?"), c("server emulato"));
        assert_eq!(capisci_prima("ti ricordi della riunione"), c("riunione"));
        assert_eq!(capisci_prima("dove ho messo l'alimentatore"), c("alimentatore"));
    }

    #[test]
    fn cortesia() {
        assert_eq!(capisci_prima("Ciao!"), Some(Intento::Saluto));
        assert_eq!(capisci_prima("ciao, come stai?"), Some(Intento::ComeStai));
        assert_eq!(capisci_prima("come va"), Some(Intento::ComeStai));
        assert_eq!(capisci_prima("chi sei?"), Some(Intento::ChiSei));
        assert_eq!(capisci_prima("grazie mille"), Some(Intento::Grazie));
        assert_eq!(capisci_prima("bais, come ti chiami?"), Some(Intento::ChiSei));
    }

    #[test]
    fn frasi_del_tema_passano_al_classificatore() {
        assert_eq!(capisci_prima("vorrei un desktop tipo mac al buio"), None);
        assert_eq!(capisci_prima("macos scuro con la barra di windows"), None);
        assert_eq!(capisci_prima("stile windows 11"), None);
    }

    #[test]
    fn domande_e_sconosciuti() {
        assert_eq!(capisci_dopo("quando è la riunione?", CMD), Intento::Domanda(String::from("riunione")));
        assert_eq!(capisci_dopo("perché il server è lento", CMD), Intento::Domanda(String::from("server lento")));
        assert_eq!(capisci_dopo("hardwre", CMD), Intento::Sconosciuto { suggerimento: Some("hardware") });
        assert_eq!(capisci_dopo("ricrda il gatto", CMD), Intento::Sconosciuto { suggerimento: Some("ricorda") });
        assert_eq!(capisci_dopo("spengi", CMD), Intento::Sconosciuto { suggerimento: Some("spegni") });
        assert_eq!(capisci_dopo("mi passi il sale", CMD), Intento::Sconosciuto { suggerimento: None });
    }

    #[test]
    fn normalizzazione() {
        assert_eq!(normalizza("  Aiuto.  "), "aiuto");
        assert_eq!(normalizza("HARDWARE!!"), "hardware");
        assert_eq!(normalizza("il server e' emulato"), "il server è emulato");
        assert_eq!(normalizza("Perche'   no?"), "perché no");
        assert_eq!(parole_chiave("Dov'è L'Alimentatore del PC?"), "alimentatore");
    }
}
