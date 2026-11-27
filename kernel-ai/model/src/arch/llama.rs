//! Architettura llama: GQA, RoPE normale (coppie adiacenti), SwiGLU, MoE opzionale (stile Mixtral).

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use libm::{powf, sincosf, sqrtf};

use crate::gguf::{Gguf, Tensor};
use crate::ops::{add, rmsnorm, silu, softmax};
use crate::quant::{get_row, matvec, matvec_new, to_f32};
use crate::Error;

struct Layer<'a> {
    attn_norm: Vec<f32>,
    wq: Tensor<'a>,
    wk: Tensor<'a>,
    wv: Tensor<'a>,
    wo: Tensor<'a>,
    ffn_norm: Vec<f32>,
    ffn: Ffn<'a>,
}

enum Ffn<'a> {
    Dense { gate: Tensor<'a>, up: Tensor<'a>, down: Tensor<'a> },
    Moe { router: Tensor<'a>, gate: Tensor<'a>, up: Tensor<'a>, down: Tensor<'a>, n_used: usize },
}

pub struct Llama<'a> {
    n_embd: usize,
    n_head: usize,
    n_head_kv: usize,
    head_dim: usize,
    rope_dim: usize,
    rope_base: f32,
    eps: f32,
    pub n_ctx: usize,
    embd: Tensor<'a>,
    layers: Vec<Layer<'a>>,
    out_norm: Vec<f32>,
    output: Tensor<'a>,
}

pub struct LlamaState {
    k: Vec<Vec<f32>>, // per layer: [pos][n_head_kv*head_dim]
    v: Vec<Vec<f32>>,
    pub pos: usize,
}

impl<'a> Llama<'a> {
    pub fn load(g: &Gguf<'a>) -> Result<Llama<'a>, Error> {
        let n_embd = g.u("embedding_length")?;
        let n_head = g.u("attention.head_count")?;
        let n_head_kv = g.u("attention.head_count_kv").unwrap_or(n_head);
        let head_dim = n_embd / n_head;
        let rope_dim = g.u("rope.dimension_count").unwrap_or(head_dim);
        let n_layer = g.u("block_count")?;
        let n_expert = g.u("expert_count").unwrap_or(0);
        let n_used = g.u("expert_used_count").unwrap_or(0);
        let mut layers = Vec::with_capacity(n_layer);
        for i in 0..n_layer {
            let t = |n: &str| g.tensor(&format!("blk.{}.{}.weight", i, n));
            let ffn = if n_expert > 0 {
                Ffn::Moe { router: t("ffn_gate_inp")?, gate: t("ffn_gate_exps")?, up: t("ffn_up_exps")?, down: t("ffn_down_exps")?, n_used }
            } else {
                Ffn::Dense { gate: t("ffn_gate")?, up: t("ffn_up")?, down: t("ffn_down")? }
            };
            layers.push(Layer {
                attn_norm: to_f32(&t("attn_norm")?),
                wq: t("attn_q")?,
                wk: t("attn_k")?,
                wv: t("attn_v")?,
                wo: t("attn_output")?,
                ffn_norm: to_f32(&t("ffn_norm")?),
                ffn,
            });
        }
        let embd = g.tensor("token_embd.weight")?;
        let output = g.try_tensor("output.weight").unwrap_or(embd);
        Ok(Llama {
            n_embd,
            n_head,
            n_head_kv,
            head_dim,
            rope_dim,
            rope_base: g.f("rope.freq_base").unwrap_or(10000.0),
            eps: g.f("attention.layer_norm_rms_epsilon").unwrap_or(1e-5),
            n_ctx: g.u("context_length").unwrap_or(2048),
            embd,
            layers,
            out_norm: to_f32(&g.tensor("output_norm.weight")?),
            output,
        })
    }

    pub fn new_state(&self) -> LlamaState {
        LlamaState { k: vec![Vec::new(); self.layers.len()], v: vec![Vec::new(); self.layers.len()], pos: 0 }
    }

    fn rope(&self, x: &mut [f32], n_heads: usize, pos: usize) {
        let theta_scale = powf(self.rope_base, -2.0 / self.rope_dim as f32);
        for h in 0..n_heads {
            let v = &mut x[h * self.head_dim..(h + 1) * self.head_dim];
            let mut theta = pos as f32;
            let mut i = 0;
            while i < self.rope_dim {
                let (s, c) = sincosf(theta);
                let (a, b) = (v[i], v[i + 1]);
                v[i] = a * c - b * s;
                v[i + 1] = a * s + b * c;
                theta *= theta_scale;
                i += 2;
            }
        }
    }

    /// Elabora un token alla posizione corrente e restituisce i logit.
    pub fn forward(&self, st: &mut LlamaState, token: u32) -> Vec<f32> {
        let pos = st.pos;
        let kvd = self.n_head_kv * self.head_dim;
        let mut x = get_row(&self.embd, token as usize);
        let mut h = vec![0.0f32; self.n_embd];
        for (li, l) in self.layers.iter().enumerate() {
            h.copy_from_slice(&x);
            rmsnorm(&mut h, Some(&l.attn_norm), self.eps);
            let mut q = matvec_new(&l.wq, &h);
            let mut k = matvec_new(&l.wk, &h);
            let v = matvec_new(&l.wv, &h);
            self.rope(&mut q, self.n_head, pos);
            self.rope(&mut k, self.n_head_kv, pos);
            st.k[li].extend_from_slice(&k);
            st.v[li].extend_from_slice(&v);
            let n_kv = pos + 1;
            let group = self.n_head / self.n_head_kv;
            let scale = 1.0 / sqrtf(self.head_dim as f32);
            let mut att_out = vec![0.0f32; self.n_head * self.head_dim];
            let mut sc = vec![0.0f32; n_kv];
            for hh in 0..self.n_head {
                let kh = hh / group;
                let qh = &q[hh * self.head_dim..(hh + 1) * self.head_dim];
                for t in 0..n_kv {
                    let kt = &st.k[li][t * kvd + kh * self.head_dim..t * kvd + (kh + 1) * self.head_dim];
                    let mut s = 0.0f32;
                    for d in 0..self.head_dim {
                        s += qh[d] * kt[d];
                    }
                    sc[t] = s * scale;
                }
                softmax(&mut sc);
                let o = &mut att_out[hh * self.head_dim..(hh + 1) * self.head_dim];
                for t in 0..n_kv {
                    let vt = &st.v[li][t * kvd + kh * self.head_dim..t * kvd + (kh + 1) * self.head_dim];
                    for d in 0..self.head_dim {
                        o[d] += sc[t] * vt[d];
                    }
                }
            }
            let ao = matvec_new(&l.wo, &att_out);
            add(&mut x, &ao);
            h.copy_from_slice(&x);
            rmsnorm(&mut h, Some(&l.ffn_norm), self.eps);
            let f = match &l.ffn {
                Ffn::Dense { gate, up, down } => swiglu(gate, up, down, &h),
                Ffn::Moe { router, gate, up, down, n_used } => {
                    let mut p = matvec_new(router, &h);
                    softmax(&mut p);
                    let top = top_k(&p, *n_used);
                    let wsum: f32 = top.iter().map(|&e| p[e]).sum();
                    let mut acc = vec![0.0f32; self.n_embd];
                    for &e in &top {
                        let y = swiglu(&gate.slice2(e), &up.slice2(e), &down.slice2(e), &h);
                        let wgt = p[e] / wsum;
                        for (a, b) in acc.iter_mut().zip(&y) {
                            *a += wgt * b;
                        }
                    }
                    acc
                }
            };
            add(&mut x, &f);
        }
        rmsnorm(&mut x, Some(&self.out_norm), self.eps);
        st.pos += 1;
        matvec_new(&self.output, &x)
    }
}

fn swiglu(gate: &Tensor, up: &Tensor, down: &Tensor, h: &[f32]) -> Vec<f32> {
    let g = matvec_new(gate, h);
    let u = matvec_new(up, h);
    let a: Vec<f32> = g.iter().zip(&u).map(|(&g, &u)| silu(g) * u).collect();
    let mut y = vec![0.0; down.ne[1]];
    matvec(down, &a, &mut y);
    y
}

pub fn top_k(p: &[f32], k: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..p.len()).collect();
    idx.sort_by(|&a, &b| p[b].partial_cmp(&p[a]).unwrap_or(core::cmp::Ordering::Equal).then(a.cmp(&b)));
    idx.truncate(k);
    idx
}
