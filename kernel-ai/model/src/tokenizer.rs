//! Tokenizer letto dal GGUF. Per ora: SentencePiece (modello "llama"); il BPE byte-level
//! (gpt2 / tiktoken) arriva con i modelli che lo usano.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::gguf::Gguf;
use crate::Error;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Spm,
    Bpe,
}

pub struct Tokenizer {
    pub kind: Kind,
    pub tokens: Vec<String>,
    pub scores: Vec<f32>,
    pub types: Vec<u32>,
    pub index: BTreeMap<String, u32>,
    pub merges: BTreeMap<(String, String), u32>,
    pub bos: Option<u32>,
    pub eos: Option<u32>,
    pub unk: Option<u32>,
    pub add_bos: bool,
    pub add_space_prefix: bool,
    pub ignore_merges: bool,
}

const SP: &str = "\u{2581}";

impl Tokenizer {
    pub fn from_gguf(g: &Gguf) -> Result<Tokenizer, Error> {
        let model = g.get("tokenizer.ggml.model").and_then(|v| v.as_str()).unwrap_or("llama");
        let kind = match model {
            "llama" => Kind::Spm,
            "gpt2" => Kind::Bpe,
            _ => return Err(Error::Unsupported("tokenizer")),
        };
        let toks = g.get("tokenizer.ggml.tokens").and_then(|v| v.as_arr()).ok_or(Error::Missing("tokenizer.ggml.tokens".into()))?;
        let tokens: Vec<String> = toks.iter().map(|v| String::from(v.as_str().unwrap_or(""))).collect();
        let scores: Vec<f32> = match g.get("tokenizer.ggml.scores").and_then(|v| v.as_arr()) {
            Some(a) => a.iter().map(|v| v.as_f32().unwrap_or(0.0)).collect(),
            None => alloc::vec![0.0; tokens.len()],
        };
        let types: Vec<u32> = match g.get("tokenizer.ggml.token_type").and_then(|v| v.as_arr()) {
            Some(a) => a.iter().map(|v| v.as_u64().unwrap_or(1) as u32).collect(),
            None => alloc::vec![1; tokens.len()],
        };
        let mut index = BTreeMap::new();
        for (i, t) in tokens.iter().enumerate() {
            index.entry(t.clone()).or_insert(i as u32);
        }
        let mut merges = BTreeMap::new();
        if let Some(a) = g.get("tokenizer.ggml.merges").and_then(|v| v.as_arr()) {
            for (rank, m) in a.iter().enumerate() {
                if let Some(s) = m.as_str() {
                    if let Some((l, r)) = s.split_once(' ') {
                        merges.insert((String::from(l), String::from(r)), rank as u32);
                    }
                }
            }
        }
        let pre = g.get("tokenizer.ggml.pre").and_then(|v| v.as_str()).unwrap_or("");
        let id = |k: &str| g.get(k).and_then(|v| v.as_u64()).map(|v| v as u32);
        let flag = |k: &str, d: bool| g.get(k).and_then(|v| v.as_u64()).map(|v| v != 0).unwrap_or(d);
        Ok(Tokenizer {
            kind,
            bos: id("tokenizer.ggml.bos_token_id"),
            eos: id("tokenizer.ggml.eos_token_id"),
            unk: id("tokenizer.ggml.unknown_token_id"),
            add_bos: flag("tokenizer.ggml.add_bos_token", kind == Kind::Spm) && pre != "inkling2",
            ignore_merges: pre == "inkling2",
            add_space_prefix: flag("tokenizer.ggml.add_space_prefix", kind == Kind::Spm),
            tokens,
            scores,
            types,
            index,
            merges,
        })
    }

    pub fn n_vocab(&self) -> usize {
        self.tokens.len()
    }

    pub fn encode(&self, text: &str) -> Vec<u32> {
        let mut out = Vec::new();
        if self.add_bos {
            if let Some(b) = self.bos {
                out.push(b);
            }
        }
        match self.kind {
            Kind::Spm => self.encode_spm(text, &mut out),
            Kind::Bpe => self.encode_bpe(text, &mut out),
        }
        out
    }

    fn encode_spm(&self, text: &str, out: &mut Vec<u32>) {
        if text.is_empty() {
            return;
        }
        let mut norm = String::new();
        if self.add_space_prefix {
            norm.push_str(SP);
        }
        for c in text.chars() {
            if c == ' ' {
                norm.push_str(SP);
            } else {
                norm.push(c);
            }
        }
        // simboli = caratteri UTF-8; si fonde ogni volta la coppia col punteggio più alto
        let mut sym: Vec<String> = norm.chars().map(|c| {
            let mut s = String::new();
            s.push(c);
            s
        }).collect();
        loop {
            let mut best: Option<(usize, f32)> = None;
            for i in 0..sym.len().saturating_sub(1) {
                let mut m = sym[i].clone();
                m.push_str(&sym[i + 1]);
                if let Some(&id) = self.index.get(&m) {
                    let sc = self.scores[id as usize];
                    if best.map(|(_, b)| sc > b).unwrap_or(true) {
                        best = Some((i, sc));
                    }
                }
            }
            match best {
                Some((i, _)) => {
                    let r = sym.remove(i + 1);
                    sym[i].push_str(&r);
                }
                None => break,
            }
        }
        for s in sym {
            if let Some(&id) = self.index.get(&s) {
                out.push(id);
            } else {
                // ripiego sui token byte <0xXX>
                let mut ok = true;
                let mut ids = Vec::new();
                for b in s.as_bytes() {
                    let name = alloc::format!("<0x{:02X}>", b);
                    match self.index.get(&name) {
                        Some(&id) => ids.push(id),
                        None => ok = false,
                    }
                }
                if ok {
                    out.extend(ids);
                } else if let Some(u) = self.unk {
                    out.push(u);
                }
            }
        }
    }

    fn encode_bpe(&self, text: &str, out: &mut Vec<u32>) {
        // BPE byte-level: pre-split in stile GPT-4o (usato da Inkling 2), poi fusioni per rango.
        for chunk in crate::pretok::split_gpt4o(text) {
            let enc: String = chunk.as_bytes().iter().map(|&b| byte_to_unicode(b)).collect();
            if self.ignore_merges {
                if let Some(&id) = self.index.get(&enc) {
                    out.push(id);
                    continue;
                }
            }
            let mut sym: Vec<String> = chunk.as_bytes().iter().map(|&b| byte_to_unicode(b)).collect();
            loop {
                let mut best: Option<(usize, u32)> = None;
                for i in 0..sym.len().saturating_sub(1) {
                    if let Some(&r) = self.merges.get(&(sym[i].clone(), sym[i + 1].clone())) {
                        if best.map(|(_, b)| r < b).unwrap_or(true) {
                            best = Some((i, r));
                        }
                    }
                }
                match best {
                    Some((i, _)) => {
                        let r = sym.remove(i + 1);
                        sym[i].push_str(&r);
                    }
                    None => break,
                }
            }
            for s in sym {
                if let Some(&id) = self.index.get(&s) {
                    out.push(id);
                }
            }
        }
    }

    pub fn piece(&self, id: u32) -> String {
        let t = match self.tokens.get(id as usize) {
            Some(t) => t,
            None => return String::new(),
        };
        match self.kind {
            Kind::Spm => {
                if self.types.get(id as usize) == Some(&6) && t.len() == 6 && t.starts_with("<0x") {
                    if let Ok(b) = u8::from_str_radix(&t[3..5], 16) {
                        return String::from_utf8_lossy(&[b]).into_owned();
                    }
                }
                t.replace(SP, " ")
            }
            Kind::Bpe => {
                let bytes: Vec<u8> = t.chars().filter_map(unicode_to_byte).collect();
                String::from_utf8_lossy(&bytes).into_owned()
            }
        }
    }

    pub fn decode(&self, ids: &[u32]) -> String {
        let mut s = String::new();
        for &i in ids {
            s.push_str(&self.piece(i));
        }
        if self.kind == Kind::Spm && self.add_space_prefix && s.starts_with(' ') {
            s.remove(0);
        }
        s
    }
}

// mappatura byte <-> unicode di GPT-2
fn byte_to_unicode(b: u8) -> String {
    let c = match b {
        b'!'..=b'~' | 0xa1..=0xac | 0xae..=0xff => b as u32,
        _ => {
            let mut n = 0u32;
            for x in 0u32..256 {
                let printable = (33..=126).contains(&x) || (0xa1..=0xac).contains(&x) || (0xae..=0xff).contains(&x);
                if !printable {
                    if x == b as u32 {
                        break;
                    }
                    n += 1;
                }
            }
            256 + n
        }
    };
    let mut s = String::new();
    s.push(char::from_u32(c).unwrap_or('?'));
    s
}

fn unicode_to_byte(c: char) -> Option<u8> {
    let u = c as u32;
    if (33..=126).contains(&u) || (0xa1..=0xac).contains(&u) || (0xae..=0xff).contains(&u) {
        return Some(u as u8);
    }
    if u >= 256 {
        let mut n = 0u32;
        for x in 0u32..256 {
            let printable = (33..=126).contains(&x) || (0xa1..=0xac).contains(&x) || (0xae..=0xff).contains(&x);
            if !printable {
                if n == u - 256 {
                    return Some(x as u8);
                }
                n += 1;
            }
        }
    }
    None
}
