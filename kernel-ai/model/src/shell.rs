//! Comandi di terminale del gestore modelli, pronti da agganciare all'interprete del kernel:
//! `modelli`, `carica <nome|n>`, `scarica <nome|n>`, `predefinito <nome|n>`, `genera <testo>`.
//! Restituiscono testo da stampare; non dipendono da driver.

use alloc::format;
use alloc::string::String;

use crate::manager::{Manager, Status, Task};
use crate::sampler::Params;

fn pick(m: &Manager, arg: &str) -> Option<usize> {
    let a = arg.trim();
    if let Ok(n) = a.parse::<usize>() {
        if n >= 1 && n <= m.list().count() {
            return Some(n - 1);
        }
    }
    m.find(a)
}

fn params(n: usize) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2} miliardi di parametri", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.1} milioni di parametri", n as f64 / 1e6)
    } else {
        format!("{} mila parametri", (n + 500) / 1000)
    }
}

fn mb(b: usize) -> String {
    if b >= 1 << 30 {
        format!("{:.1} GB", b as f64 / (1u64 << 30) as f64)
    } else {
        format!("{:.1} MB", b as f64 / (1u64 << 20) as f64)
    }
}

/// Esegue un comando; `None` se la frase non è un comando del gestore modelli.
pub fn run(m: &mut Manager, line: &str, max_tokens: usize, mut out: impl FnMut(&str)) -> Option<String> {
    let line = line.trim();
    let (cmd, arg) = match line.split_once(' ') {
        Some((c, a)) => (c, a.trim()),
        None => (line, ""),
    };
    match cmd {
        "modelli" => {
            let mut s = String::new();
            for (i, inf) in m.list().enumerate() {
                let st = match inf.status {
                    Status::Registered => "registrato",
                    Status::Loaded => "caricato",
                    Status::Busy => "in uso",
                };
                s.push_str(&format!(
                    "{}. {}{}  [{} {}]  {}  {}  {}{}\n",
                    i + 1,
                    inf.name,
                    if inf.default { " *" } else { "" },
                    inf.arch,
                    inf.quant,
                    mb(inf.bytes),
                    params(inf.n_params),
                    st,
                    if inf.caps.classify { "  (classificatore)" } else { "" }
                ));
            }
            if s.is_empty() {
                s.push_str("nessun modello registrato\n");
            }
            s.push_str(&format!("memoria modelli: {} su {}\n", mb(m.used_bytes()), mb(m.budget)));
            Some(s)
        }
        "carica" => Some(match pick(m, arg) {
            None => String::from("modello non trovato: usa `modelli`"),
            Some(id) => match m.load(id) {
                Ok(()) => format!("caricato. memoria modelli: {}", mb(m.used_bytes())),
                Err(e) => format!("errore: {}", e),
            },
        }),
        "scarica" => Some(match pick(m, arg) {
            None => String::from("modello non trovato"),
            Some(id) => match m.unload(id) {
                Ok(()) => String::from("scaricato"),
                Err(e) => format!("errore: {}", e),
            },
        }),
        "predefinito" => Some(match pick(m, arg) {
            None => String::from("modello non trovato"),
            Some(id) => match m.set_default(id) {
                Ok(()) => String::from("modello predefinito aggiornato"),
                Err(e) => format!("errore: {}", e),
            },
        }),
        "genera" => Some(match m.choose(Task::Generate) {
            None => String::from("nessun modello caricato: usa `carica`"),
            Some(id) => match m.generate(id, arg, max_tokens, Params::INKLING, |_, p| {
                out(p);
                true
            }) {
                Ok(o) => format!("\n[{} token]", o.tokens.len()),
                Err(e) => format!("errore: {}", e),
            },
        }),
        // collaudo: id del prompt e dei token greedy, nello stesso formato del riferimento llama.cpp
        "token" => Some(match m.choose(Task::Generate) {
            None => String::from("nessun modello caricato: usa `carica`"),
            Some(id) => {
                let (n, text) = match arg.split_once(' ') {
                    Some((a, t)) if a.parse::<usize>().is_ok() => (a.parse::<usize>().unwrap_or(32), t),
                    _ => (32, arg),
                };
                match (m.tokenize(id, text), m.generate(id, text, n, Params::GREEDY, |_, _| true)) {
                    (Ok(p), Ok(o)) => {
                        let mut s = String::from("prompt:");
                        for t in &p {
                            s.push_str(&format!(" {}", t));
                        }
                        s.push_str("\ngen:");
                        for t in &o.tokens {
                            s.push_str(&format!(" {}", t));
                        }
                        s
                    }
                    (Err(e), _) | (_, Err(e)) => format!("errore: {}", e),
                }
            }
        }),
        _ => None,
    }
}
