//! Gestore modelli del kernel: registro, caricamento, scheduler, generazione.
//!
//! Il kernel registra i buffer dei modelli che conosce (moduli GRUB oggi, file sul disco
//! dalla Fase 4). `carica` interpreta il GGUF e prepara il motore senza copiare i pesi;
//! lo scheduler sceglie quale modello caricato usare per ogni richiesta.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::arch::{self, Net, State};
use crate::gguf::{DType, Gguf};
use crate::sampler::{Params, Sampler};
use crate::tokenizer::Tokenizer;
use crate::Error;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Registered,
    Loaded,
    Busy,
}

/// Che tipo di lavoro serve allo scheduler.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Task {
    /// classificare una frase (modelli piccoli e veloci)
    Classify,
    /// generare testo, codice, interfacce (il modello principale)
    Generate,
    /// capire un'immagine
    Vision,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Caps {
    pub text: bool,
    pub vision: bool,
    pub tools: bool,
    /// classificatore degli intenti (chiave GGUF `bais.role` = "intent")
    pub classify: bool,
}

pub struct Info {
    pub name: String,
    pub arch: String,
    pub quant: &'static str,
    pub bytes: usize,
    pub n_params: usize,
    pub caps: Caps,
    pub status: Status,
    pub default: bool,
}

struct Loaded<'a> {
    net: Net<'a>,
    tok: Tokenizer,
    work_bytes: usize,
}

struct Slot<'a> {
    info: Info,
    buf: &'a [u8],
    loaded: Option<Box<Loaded<'a>>>,
}

pub struct Manager<'a> {
    slots: Vec<Slot<'a>>,
    /// limite di memoria per i modelli caricati (pesi + strutture), in byte
    pub budget: usize,
}

pub struct Output {
    pub tokens: Vec<u32>,
    pub text: String,
    pub prompt_tokens: usize,
}

impl<'a> Manager<'a> {
    pub fn new(budget: usize) -> Manager<'a> {
        Manager { slots: Vec::new(), budget }
    }

    /// Registra un modello a partire dal suo buffer GGUF (legge solo i metadati).
    pub fn register(&mut self, buf: &'a [u8]) -> Result<usize, Error> {
        let g = Gguf::parse(buf)?;
        let arch = String::from(g.arch());
        let name = g.get("general.name").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| arch.clone());
        let mut n_params = 0usize;
        let mut hist: [(DType, usize); 6] = [(DType::F32, 0), (DType::F16, 0), (DType::Q4_0, 0), (DType::Q8_0, 0), (DType::IQ4NL, 0), (DType::Other(0), 0)];
        for t in &g.tensors {
            let n: usize = t.ne.iter().product();
            n_params += n;
            if t.n_dims >= 2 {
                let i = match t.dtype {
                    DType::F32 => 0,
                    DType::F16 => 1,
                    DType::Q4_0 => 2,
                    DType::Q8_0 => 3,
                    DType::IQ4NL => 4,
                    DType::Other(_) => 5,
                };
                hist[i].1 += n;
            }
        }
        let quant = hist.iter().max_by_key(|h| h.1).map(|h| h.0.name()).unwrap_or("?");
        let caps = Caps {
            text: true,
            vision: g.get("clip.has_vision_encoder").is_some(),
            tools: g.get("tokenizer.chat_template").and_then(|v| v.as_str()).map(|s| s.contains("tool")).unwrap_or(false),
            classify: g.get("bais.role").and_then(|v| v.as_str()) == Some("intent"),
        };
        // il classificatore non diventa mai il modello predefinito per generare
        let first = !caps.classify && !self.slots.iter().any(|s| s.info.default);
        self.slots.push(Slot {
            info: Info { name, arch, quant, bytes: buf.len(), n_params, caps, status: Status::Registered, default: first },
            buf,
            loaded: None,
        });
        Ok(self.slots.len() - 1)
    }

    pub fn list(&self) -> impl Iterator<Item = &Info> {
        self.slots.iter().map(|s| &s.info)
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.slots.iter().position(|s| s.info.name == name)
    }

    pub fn used_bytes(&self) -> usize {
        self.slots.iter().filter_map(|s| s.loaded.as_ref().map(|l| s.info.bytes + l.work_bytes)).sum()
    }

    pub fn load(&mut self, id: usize) -> Result<(), Error> {
        let need = {
            let s = self.slots.get(id).ok_or(Error::NotFound)?;
            if s.loaded.is_some() {
                return Ok(());
            }
            if !arch::supported(&s.info.arch) {
                return Err(Error::Unsupported("architettura"));
            }
            s.info.bytes
        };
        if self.used_bytes() + need > self.budget {
            return Err(Error::NoMemory);
        }
        let s = &mut self.slots[id];
        let g = Gguf::parse(s.buf)?;
        let tok = Tokenizer::from_gguf(&g)?;
        let net = Net::load(&g)?;
        // stima delle strutture di lavoro: norme in f32 + vocabolario
        let work_bytes = tok.tokens.iter().map(|t| t.len() + 32).sum::<usize>();
        s.loaded = Some(Box::new(Loaded { net, tok, work_bytes }));
        s.info.status = Status::Loaded;
        Ok(())
    }

    pub fn unload(&mut self, id: usize) -> Result<(), Error> {
        let s = self.slots.get_mut(id).ok_or(Error::NotFound)?;
        s.loaded = None;
        s.info.status = Status::Registered;
        Ok(())
    }

    pub fn set_default(&mut self, id: usize) -> Result<(), Error> {
        if id >= self.slots.len() {
            return Err(Error::NotFound);
        }
        for (i, s) in self.slots.iter_mut().enumerate() {
            s.info.default = i == id;
        }
        Ok(())
    }

    /// Scheduler: sceglie il modello caricato più adatto al compito.
    pub fn choose(&self, task: Task) -> Option<usize> {
        let loaded: Vec<usize> = (0..self.slots.len()).filter(|&i| self.slots[i].loaded.is_some()).collect();
        if loaded.is_empty() {
            return None;
        }
        match task {
            Task::Classify => loaded
                .iter()
                .copied()
                .filter(|&i| self.slots[i].info.caps.classify)
                .min_by_key(|&i| self.slots[i].info.n_params),
            Task::Generate => loaded
                .iter()
                .copied()
                .find(|&i| self.slots[i].info.default)
                .or_else(|| loaded.iter().copied().filter(|&i| !self.slots[i].info.caps.classify).max_by_key(|&i| self.slots[i].info.n_params))
                .or_else(|| loaded.iter().copied().max_by_key(|&i| self.slots[i].info.n_params)),
            Task::Vision => loaded.iter().copied().find(|&i| self.slots[i].info.caps.vision),
        }
    }

    /// Classifica una frase con il classificatore degli intenti caricato: restituisce le
    /// etichette prodotte in modo greedy (es. `#macos #scuro`), separate da spazi.
    pub fn classify(&mut self, text: &str, max_tokens: usize) -> Option<(usize, String)> {
        let id = self.choose(Task::Classify)?;
        let o = self.generate(id, text, max_tokens, Params::GREEDY, |_, _| true).ok()?;
        Some((id, o.text))
    }

    pub fn name(&self, id: usize) -> &str {
        self.slots.get(id).map(|s| s.info.name.as_str()).unwrap_or("?")
    }

    pub fn tokenize(&self, id: usize, text: &str) -> Result<Vec<u32>, Error> {
        let l = self.slots.get(id).ok_or(Error::NotFound)?.loaded.as_ref().ok_or(Error::NotLoaded)?;
        Ok(l.tok.encode(text))
    }

    /// Genera fino a `max_tokens` token (il limite è sempre obbligatorio: alcuni modelli
    /// possono entrare in loop nel ragionamento). `on_token` riceve ogni pezzo di testo;
    /// se restituisce false la generazione si ferma.
    pub fn generate(
        &mut self,
        id: usize,
        prompt: &str,
        max_tokens: usize,
        params: Params,
        mut on_token: impl FnMut(u32, &str) -> bool,
    ) -> Result<Output, Error> {
        let s = self.slots.get_mut(id).ok_or(Error::NotFound)?;
        let l = s.loaded.as_ref().ok_or(Error::NotLoaded)?;
        s.info.status = Status::Busy;
        let ids = l.tok.encode(prompt);
        let mut st: State = l.net.new_state();
        let mut sampler = Sampler::new(params);
        let mut logits = Vec::new();
        for &t in &ids {
            logits = l.net.forward(&mut st, t);
        }
        crate::trace("first_logits", 0, &logits);
        let mut out = Vec::new();
        let n_ctx = l.net.n_ctx();
        for _ in 0..max_tokens {
            if ids.len() + out.len() >= n_ctx || logits.is_empty() {
                break;
            }
            let t = sampler.sample(&logits[..l.tok.n_vocab().min(logits.len())]);
            if Some(t) == l.tok.eos {
                break;
            }
            out.push(t);
            if !on_token(t, &l.tok.piece(t)) {
                break;
            }
            logits = l.net.forward(&mut st, t);
        }
        let text = l.tok.decode(&out);
        s.info.status = Status::Loaded;
        Ok(Output { prompt_tokens: ids.len(), tokens: out, text })
    }
}
