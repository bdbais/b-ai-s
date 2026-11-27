//! Pre-split di GPT-4o (la regex LLAMA_VOCAB_PRE_TYPE_GPT4O di llama.cpp, usata da Inkling 2),
//! scritto a mano: nel kernel non c'è un motore di regex.

use alloc::string::String;
use alloc::vec::Vec;

fn is_letter(c: char) -> bool {
    c.is_alphabetic()
}

fn is_upper(c: char) -> bool {
    c.is_uppercase()
}

/// \p{Ll}\p{Lm}\p{Lo}: lettere che non sono maiuscole
fn is_lower(c: char) -> bool {
    c.is_alphabetic() && !c.is_uppercase()
}

fn is_nl(c: char) -> bool {
    c == '\r' || c == '\n'
}

pub fn split_gpt4o(text: &str) -> Vec<&str> {
    let cs: Vec<(usize, char)> = text.char_indices().collect();
    let n = cs.len();
    let at = |i: usize| cs[i].1;
    let off = |i: usize| if i < n { cs[i].0 } else { text.len() };
    // (?i:'s|'t|'re|'ve|'m|'ll|'d)
    let contraction = |i: usize| -> usize {
        if i < n && at(i) == '\'' {
            let rest: String = cs[i + 1..n.min(i + 3)].iter().map(|c| c.1.to_ascii_lowercase()).collect();
            for (s, l) in [("re", 2), ("ve", 2), ("ll", 2), ("s", 1), ("t", 1), ("m", 1), ("d", 1)] {
                if rest.starts_with(s) {
                    return 1 + l;
                }
            }
        }
        0
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let c = at(i);
        // [^\r\n\p{L}\p{N}]? (Maiuscole* minuscole+ | Maiuscole+ minuscole*) contrazione?
        let pre = if !is_letter(c) && !c.is_numeric() && !is_nl(c) { 1 } else { 0 };
        let s = i + pre;
        if s < n && is_letter(at(s)) {
            let mut j = s;
            while j < n && is_upper(at(j)) {
                j += 1;
            }
            let ups = j - s;
            let mut k = j;
            while k < n && is_lower(at(k)) {
                k += 1;
            }
            // primo ramo: le maiuscole iniziali sono seguite da minuscole; altrimenti
            // secondo ramo (Maiuscole+ minuscole*), che qui coincide
            if k > j || ups > 0 {
                let mut e = k;
                e += contraction(e);
                out.push(&text[off(i)..off(e)]);
                i = e;
                continue;
            }
        }
        // \p{N}{1,3}
        if c.is_numeric() {
            let mut j = i;
            while j < n && j < i + 3 && at(j).is_numeric() {
                j += 1;
            }
            out.push(&text[off(i)..off(j)]);
            i = j;
            continue;
        }
        // ' ?[^\s\p{L}\p{N}]+[\r\n/]*'
        {
            let mut j = i;
            if at(j) == ' ' {
                j += 1;
            }
            let st = j;
            while j < n && !at(j).is_whitespace() && !is_letter(at(j)) && !at(j).is_numeric() {
                j += 1;
            }
            if j > st {
                while j < n && (is_nl(at(j)) || at(j) == '/') {
                    j += 1;
                }
                out.push(&text[off(i)..off(j)]);
                i = j;
                continue;
            }
        }
        // spazi: \s*[\r\n]+ | \s+(?!\S) | \s+
        if c.is_whitespace() {
            let mut j = i;
            while j < n && at(j).is_whitespace() {
                j += 1;
            }
            let mut last_nl = None;
            for k in i..j {
                if is_nl(at(k)) {
                    last_nl = Some(k);
                }
            }
            if let Some(k) = last_nl {
                out.push(&text[off(i)..off(k + 1)]);
                i = k + 1;
                continue;
            }
            if j < n && j - i > 1 {
                out.push(&text[off(i)..off(j - 1)]);
                i = j - 1;
                continue;
            }
            out.push(&text[off(i)..off(j)]);
            i = j;
            continue;
        }
        out.push(&text[off(i)..off(i + 1)]);
        i += 1;
    }
    out
}
