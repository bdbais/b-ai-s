//! bais-model — gestore modelli e motore di inferenza di kernel-ai (B-AI-S).
//!
//! `no_std` + `alloc`: lo stesso codice gira nel kernel bare metal e nello strumento
//! di collaudo su host. I pesi non vengono copiati: il motore lavora sul buffer GGUF
//! (modulo caricato in RAM o, dalla Fase 4, file mappato dal disco).

#![no_std]
extern crate alloc;

pub mod arch;
pub mod gguf;
pub mod manager;
pub mod ops;
pub mod pretok;
pub mod quant;
pub mod sampler;
pub mod shell;
pub mod simd;
pub mod tokenizer;

use alloc::string::String;

#[derive(Debug)]
pub enum Error {
    Format(&'static str),
    Missing(String),
    Unsupported(&'static str),
    NotFound,
    NotLoaded,
    NoMemory,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Format(s) => write!(f, "formato: {}", s),
            Error::Missing(s) => write!(f, "manca: {}", s),
            Error::Unsupported(s) => write!(f, "non supportato: {}", s),
            Error::NotFound => write!(f, "modello non trovato"),
            Error::NotLoaded => write!(f, "modello non caricato"),
            Error::NoMemory => write!(f, "memoria insufficiente"),
        }
    }
}

/// Gancio di diagnostica: se impostato, il motore vi passa i tensori intermedi
/// (nome, layer, valori). Nel kernel resta vuoto.
pub static mut TRACE: Option<fn(&str, usize, &[f32])> = None;

#[inline]
pub(crate) fn trace(name: &str, layer: usize, v: &[f32]) {
    // SAFETY: impostato una sola volta prima dell'uso, solo negli strumenti di collaudo
    unsafe {
        if let Some(f) = TRACE {
            f(name, layer, v)
        }
    }
}
