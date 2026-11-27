//! Campionamento: greedy oppure temperatura + top-p + top-k + min-p con un PRNG deterministico.

use alloc::vec::Vec;
use libm::expf;

use crate::ops::argmax;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub temperature: f32, // 0 = greedy
    pub top_p: f32,
    pub top_k: usize, // 0 = disattivato
    pub min_p: f32,
    pub seed: u64,
}

impl Params {
    pub const GREEDY: Params = Params { temperature: 0.0, top_p: 1.0, top_k: 0, min_p: 0.0, seed: 0 };
    /// valori consigliati da Thinking Machines per Inkling 2 Mini
    pub const INKLING: Params = Params { temperature: 1.0, top_p: 1.0, top_k: 0, min_p: 0.0, seed: 0x5eed };
}

pub struct Sampler {
    p: Params,
    state: u64,
}

impl Sampler {
    pub fn new(p: Params) -> Sampler {
        Sampler { p, state: p.seed ^ 0x9e37_79b9_7f4a_7c15 }
    }

    fn next_f32(&mut self) -> f32 {
        // xorshift64*
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        let v = self.state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (v >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn sample(&mut self, logits: &[f32]) -> u32 {
        if self.p.temperature <= 0.0 {
            return argmax(logits) as u32;
        }
        let mut idx: Vec<usize> = (0..logits.len()).collect();
        idx.sort_by(|&a, &b| logits[b].partial_cmp(&logits[a]).unwrap_or(core::cmp::Ordering::Equal));
        if self.p.top_k > 0 {
            idx.truncate(self.p.top_k);
        }
        let m = logits[idx[0]];
        let mut pr: Vec<f32> = idx.iter().map(|&i| expf((logits[i] - m) / self.p.temperature)).collect();
        let s: f32 = pr.iter().sum();
        for v in pr.iter_mut() {
            *v /= s;
        }
        let pmax = pr[0];
        let mut keep = pr.len();
        let mut cum = 0.0;
        for (i, &v) in pr.iter().enumerate() {
            if v < self.p.min_p * pmax {
                keep = i.max(1);
                break;
            }
            cum += v;
            if cum >= self.p.top_p {
                keep = i + 1;
                break;
            }
        }
        let tot: f32 = pr[..keep].iter().sum();
        let mut r = self.next_f32() * tot;
        for i in 0..keep {
            r -= pr[i];
            if r <= 0.0 {
                return idx[i] as u32;
            }
        }
        idx[keep - 1] as u32
    }
}
