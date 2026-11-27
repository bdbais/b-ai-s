//! Architetture supportate dal motore.

pub mod inkling2;
pub mod llama;

use alloc::vec::Vec;

use crate::gguf::Gguf;
use crate::Error;

pub enum Net<'a> {
    Llama(llama::Llama<'a>),
    Inkling2(inkling2::Inkling2<'a>),
}

pub enum State {
    Llama(llama::LlamaState),
    Inkling2(inkling2::Inkling2State),
}

impl<'a> Net<'a> {
    pub fn load(g: &Gguf<'a>) -> Result<Net<'a>, Error> {
        match g.arch() {
            "llama" => Ok(Net::Llama(llama::Llama::load(g)?)),
            "inkling2" | "inkling2_loop" => Ok(Net::Inkling2(inkling2::Inkling2::load(g)?)),
            _ => Err(Error::Unsupported("architettura")),
        }
    }
    pub fn new_state(&self) -> State {
        match self {
            Net::Llama(m) => State::Llama(m.new_state()),
            Net::Inkling2(m) => State::Inkling2(m.new_state()),
        }
    }
    pub fn forward(&self, st: &mut State, token: u32) -> Vec<f32> {
        match (self, st) {
            (Net::Llama(m), State::Llama(s)) => m.forward(s, token),
            (Net::Inkling2(m), State::Inkling2(s)) => m.forward(s, token),
            _ => Vec::new(),
        }
    }
    pub fn n_ctx(&self) -> usize {
        match self {
            Net::Llama(m) => m.n_ctx,
            Net::Inkling2(m) => m.n_ctx,
        }
    }
}

pub fn supported(arch: &str) -> bool {
    matches!(arch, "llama" | "inkling2" | "inkling2_loop")
}
