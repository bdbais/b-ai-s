//! Inkling 2 (Thinking Machines) — architetture `inkling2` e `inkling2_loop` (Inkling 2 Mini).
//!
//! Porting dal riferimento llama.cpp (src/models/inkling2*.cpp):
//! - niente RoPE: bias di posizione relativa (tabella per distanza) proiettato da R, posizioni
//!   log-scalate (tau) nei layer ad attenzione piena, sink per testa;
//! - convoluzioni causali brevi su K, V, uscita dell'attenzione e uscita della FFN, con tap
//!   dinamici per testa nei layer ad attenzione piena;
//! - residui per blocchi: gli input dei layer vengono "mescolati" con query apprese sugli stati
//!   d'inizio blocco (attenzione lungo la profondità);
//! - MoE con gating sigmoide, bias di selezione, gate+up fusi e attivazione SITU;
//! - loop: il nucleo [start, end) viene rieseguito `count` volte; dalla seconda passata ogni
//!   layer riceve messaggi dalla passata precedente attraverso controllori low-rank.
//! Ogni passata del nucleo ha la propria cache KV e il proprio stato delle convoluzioni.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use libm::{logf, sqrtf, tanhf};

use crate::gguf::{Gguf, Tensor, Value};
use crate::ops::{add, rmsnorm, sigmoid, softmax};
use crate::quant::{get_row, matvec_new, to_f32};
use crate::Error;

const CTRL_Q: usize = 0;
const CTRL_FFN: usize = 3;

struct LoopCtl<'a> {
    msg_query: Vec<f32>, // [4][E]
    down: [Tensor<'a>; 4],
    gain: [Tensor<'a>; 4],
    forget: [Tensor<'a>; 4],
}

struct Layer<'a> {
    swa: bool,
    n_head: usize,
    n_head_kv: usize,
    n_used: usize,
    wq: Tensor<'a>,
    wk: Tensor<'a>,
    wv: Tensor<'a>,
    wo: Tensor<'a>,
    wgate: Tensor<'a>,
    q_clip: Vec<f32>,
    sink_proj: Tensor<'a>,
    rel: Tensor<'a>,
    rel_bias: Vec<f32>, // [extent][n_rel]
    rel_clip: Vec<f32>,
    k_conv: Vec<f32>,
    v_conv: Vec<f32>,
    post_conv: Vec<f32>,
    ffn_post_conv: Vec<f32>,
    attn_norm: Option<Vec<f32>>,
    qkv_norm: Option<[Vec<f32>; 3]>,
    k_conv_proj: Option<Tensor<'a>>,
    v_conv_proj: Option<Tensor<'a>>,
    attn_res_query: Option<(Vec<f32>, usize)>,
    ffn_res_query: Vec<f32>,
    ffn_norm: Vec<f32>,
    router: Tensor<'a>,
    probs_bias: Option<Vec<f32>>,
    gate_up: Tensor<'a>,
    down: Tensor<'a>,
    ctl: Option<LoopCtl<'a>>,
}

pub struct Inkling2<'a> {
    e: usize,
    hd: usize,
    eps: f32,
    qk_clip: f32,
    rel_clip: f32,
    n_rel: usize,
    rel_extent: usize,
    n_swa: usize,
    n_conv: usize,
    block: usize,
    log_floor: f32,
    log_alpha: f32,
    situ_beta: f32,
    situ_lin: f32,
    w_scale: f32,
    softcap: f32,
    n_vocab: usize,
    pub n_ctx: usize,
    // loop
    core: Option<(usize, usize, usize)>, // start, end, passes
    embd: Tensor<'a>,
    tok_norm: Vec<f32>,
    out_norm: Vec<f32>,
    out_res_query: Vec<f32>,
    output: Tensor<'a>,
    layers: Vec<Layer<'a>>,
}

struct Slot {
    k: Vec<f32>,
    v: Vec<f32>,
    hist: [Vec<f32>; 4], // k, v, uscita attenzione, uscita ffn: n_conv-1 ingressi passati ciascuno
}

pub struct Inkling2State {
    slots: Vec<Slot>,
    pub pos: usize,
}

fn per_layer(g: &Gguf, key: &str, il: usize, default: usize) -> usize {
    match g.get_arch(key) {
        Some(Value::Arr(a)) => a.get(il).and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(default),
        Some(v) => v.as_u64().map(|v| v as usize).unwrap_or(default),
        None => default,
    }
}

impl<'a> Inkling2<'a> {
    pub fn load(g: &Gguf<'a>) -> Result<Inkling2<'a>, Error> {
        let looped = g.arch() == "inkling2_loop";
        let e = g.u("embedding_length")?;
        let n_layer = g.u("block_count")?;
        let n_head0 = per_layer(g, "attention.head_count", 0, 0);
        let hd = g.u("attention.key_length").unwrap_or(e / n_head0.max(1));
        let n_conv = g.u("shortconv.l_cache")?;
        let n_swa = g.u("attention.sliding_window")?;
        let rel_extent = g.u("attention.relative_extent")?;
        let n_rel = g.u("attention.relative_length")?;
        let pattern: Vec<bool> = g
            .get_arch("attention.sliding_window_pattern")
            .and_then(|v| v.as_arr())
            .ok_or(Error::Missing("sliding_window_pattern".into()))?
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) != 0)
            .collect();
        if pattern.len() != n_layer {
            return Err(Error::Format("inkling2: schema dei layer non valido"));
        }
        let core = if looped {
            Some((g.u("loop.start")?, g.u("loop.end")?, g.u("loop.count")?))
        } else {
            None
        };
        let t = |n: &str| g.tensor(n);
        let mut layers = Vec::with_capacity(n_layer);
        for i in 0..n_layer {
            let bt = |n: &str| g.tensor(&format!("blk.{}.{}.weight", i, n));
            let swa = pattern[i];
            let ctl = match core {
                Some((s, en, _)) if i >= s && i < en => {
                    let c = |k: &str| bt(&format!("loop_{}", k));
                    Some(LoopCtl {
                        msg_query: to_f32(&bt("loop_msg_query")?),
                        down: [c("q_ctrl_down")?, c("k_ctrl_down")?, c("v_ctrl_down")?, c("ffn_ctrl_down")?],
                        gain: [c("q_ctrl_gain")?, c("k_ctrl_gain")?, c("v_ctrl_gain")?, c("ffn_ctrl_gain")?],
                        forget: [c("q_ctrl_forget")?, c("k_ctrl_forget")?, c("v_ctrl_forget")?, c("ffn_ctrl_forget")?],
                    })
                }
                _ => None,
            };
            let arq = if i > 0 {
                let q = bt("attn_res_query")?;
                Some((to_f32(&q), q.ne[1]))
            } else {
                None
            };
            layers.push(Layer {
                swa,
                n_head: per_layer(g, "attention.head_count", i, n_head0),
                n_head_kv: per_layer(g, "attention.head_count_kv", i, n_head0),
                n_used: per_layer(g, "expert_used_count", i, 1),
                wq: bt("attn_q")?,
                wk: bt("attn_k")?,
                wv: bt("attn_v")?,
                wo: bt("attn_output")?,
                wgate: bt("attn_gate")?,
                q_clip: to_f32(&bt("attn_q_clip")?),
                sink_proj: bt("attn_sink_proj")?,
                rel: bt("attn_rel")?,
                rel_bias: to_f32(&bt("attn_rel_bias")?),
                rel_clip: to_f32(&bt("attn_rel_clip")?),
                k_conv: to_f32(&bt("attn_k_conv")?),
                v_conv: to_f32(&bt("attn_v_conv")?),
                post_conv: to_f32(&bt("attn_post_conv")?),
                ffn_post_conv: to_f32(&bt("ffn_post_conv")?),
                attn_norm: if swa { Some(to_f32(&bt("attn_norm")?)) } else { None },
                qkv_norm: if swa {
                    None
                } else {
                    Some([to_f32(&bt("attn_q_inp_norm")?), to_f32(&bt("attn_k_inp_norm")?), to_f32(&bt("attn_v_inp_norm")?)])
                },
                k_conv_proj: if swa { None } else { Some(bt("attn_k_conv_proj")?) },
                v_conv_proj: if swa { None } else { Some(bt("attn_v_conv_proj")?) },
                attn_res_query: arq,
                ffn_res_query: to_f32(&bt("ffn_res_query")?),
                ffn_norm: to_f32(&bt("ffn_norm")?),
                router: bt("ffn_gate_inp")?,
                probs_bias: g.try_tensor(&format!("blk.{}.exp_probs_b.bias", i)).map(|t| to_f32(&t)),
                gate_up: bt("ffn_gate_up_exps")?,
                down: bt("ffn_down_exps")?,
                ctl,
            });
        }
        Ok(Inkling2 {
            e,
            hd,
            eps: g.f("attention.layer_norm_rms_epsilon")?,
            qk_clip: g.f("attention.qk_clip")?,
            rel_clip: g.f("attention.relative_clip")?,
            n_rel,
            rel_extent,
            n_swa,
            n_conv,
            block: g.u("attn_res.block_size")?,
            log_floor: g.u("attention.log_scale_floor")? as f32,
            log_alpha: g.f("attention.log_scale_alpha")?,
            situ_beta: g.f("activation.situ_beta")?,
            situ_lin: g.f("activation.situ_linear_beta")?,
            w_scale: g.f("expert_weights_scale").unwrap_or(0.0),
            softcap: g.f("final_logit_softcapping").unwrap_or(0.0),
            n_vocab: g.u("vocab_size_unpadded")?,
            n_ctx: g.u("context_length").unwrap_or(8192),
            core,
            embd: t("token_embd.weight")?,
            tok_norm: to_f32(&t("token_embd_norm.weight")?),
            out_norm: to_f32(&t("output_norm.weight")?),
            out_res_query: to_f32(&t("output_res_query.weight")?),
            output: t("output.weight")?,
            layers,
        })
    }

    fn n_slots(&self) -> usize {
        match self.core {
            Some((s, e, p)) => self.layers.len() + (p - 1) * (e - s),
            None => self.layers.len(),
        }
    }

    pub fn new_state(&self) -> Inkling2State {
        let mut slots = Vec::new();
        for i in 0..self.n_slots() {
            let l = &self.layers[self.phys(i)];
            let kv = l.n_head_kv * self.hd;
            let h = self.n_conv - 1;
            slots.push(Slot { k: Vec::new(), v: Vec::new(), hist: [vec![0.0; h * kv], vec![0.0; h * kv], vec![0.0; h * self.e], vec![0.0; h * self.e]] });
        }
        Inkling2State { slots, pos: 0 }
    }

    fn phys(&self, slot: usize) -> usize {
        let n = self.layers.len();
        match self.core {
            Some((s, e, _)) if slot >= n => s + (slot - n) % (e - s),
            _ => slot,
        }
    }

    /// attenzione lungo la profondità: mescola blocchi + cur con `nq` query apprese
    fn mix(&self, blocks: &[Vec<f32>], cur: &[f32], query: &[f32], nq: usize) -> Vec<Vec<f32>> {
        let mut states: Vec<Vec<f32>> = Vec::with_capacity(blocks.len() + 1);
        for b in blocks {
            let mut s = b.clone();
            rmsnorm(&mut s, None, 1e-6);
            states.push(s);
        }
        let mut c = cur.to_vec();
        rmsnorm(&mut c, None, 1e-6);
        states.push(c);
        let scale = 1.0 / sqrtf(self.e as f32);
        let mut out = Vec::with_capacity(nq);
        for q in 0..nq {
            let qv = &query[q * self.e..(q + 1) * self.e];
            let mut sc: Vec<f32> = states.iter().map(|s| s.iter().zip(qv).map(|(a, b)| a * b).sum::<f32>() * scale).collect();
            softmax(&mut sc);
            let mut o = vec![0.0f32; self.e];
            for (s, p) in states.iter().zip(&sc) {
                for (a, b) in o.iter_mut().zip(s) {
                    *a += p * b;
                }
            }
            out.push(o);
        }
        out
    }

    /// convoluzione causale breve con residuo; `dynamic` aggiunge tap dipendenti dall'input per testa
    fn convolve(&self, cur: Vec<f32>, w: &[f32], dynamic: Option<&[f32]>, hist: &mut Vec<f32>) -> Vec<f32> {
        let ch = cur.len();
        let nc = self.n_conv;
        let past = nc - 1;
        // x[k][c]: k = 0..past dal più vecchio, k = past è l'ingresso corrente
        let xk = |k: usize, c: usize, hist: &Vec<f32>| if k < past { hist[k * ch + c] } else { cur[c] };
        let mut out = vec![0.0f32; ch];
        for c in 0..ch {
            let mut s = 0.0f32;
            for k in 0..nc {
                s += xk(k, c, hist) * w[c * nc + k];
            }
            out[c] = s;
        }
        if let Some(d) = dynamic {
            let nh = d.len() / nc;
            let per = ch / nh;
            for lag in 0..nc {
                for c in 0..ch {
                    out[c] += xk(nc - 1 - lag, c, hist) * d[(c / per) * nc + lag];
                }
            }
        }
        let sc = 1.0 / sqrtf(((if dynamic.is_some() { 2 } else { 1 }) * nc + 1) as f32);
        let res: Vec<f32> = cur.iter().zip(&out).map(|(a, b)| (a + b) * sc).collect();
        // scorre la storia
        if past > 0 {
            hist.copy_within(ch.., 0);
            let n = hist.len();
            hist[n - ch..].copy_from_slice(&cur);
        }
        res
    }

    fn clip_rows(x: &mut [f32], row: usize, scale: Option<&[f32]>, thr: f32) {
        if let Some(s) = scale {
            for (v, g) in x.iter_mut().zip(s) {
                *v *= g;
            }
        }
        for r in x.chunks_mut(row) {
            let n = sqrtf(r.iter().map(|v| v * v).sum::<f32>()) + 1e-6;
            let f = (thr / n).clamp(0.0, 1.0);
            for v in r.iter_mut() {
                *v *= f;
            }
        }
    }

    fn control(&self, cur: &[f32], msg: &[f32], ctl: &LoopCtl, c: usize) -> Vec<f32> {
        let mut a = cur.to_vec();
        rmsnorm(&mut a, None, 1e-6);
        let mut b = msg.to_vec();
        rmsnorm(&mut b, None, 1e-6);
        a.extend_from_slice(&b);
        let d = matvec_new(&ctl.down[c], &a);
        let gain = matvec_new(&ctl.gain[c], &d);
        let forget = matvec_new(&ctl.forget[c], &d);
        (0..self.e).map(|i| cur[i] - tanhf(forget[i]) * cur[i] + tanhf(gain[i]) * msg[i]).collect()
    }

    fn layer(&self, st: &mut Inkling2State, slot: usize, ip: usize, cur: Vec<f32>, blocks: &mut Vec<Vec<f32>>, msgs: Option<&[Vec<f32>]>, tau: f32) -> Vec<f32> {
        let l = &self.layers[ip];
        let pos = st.pos;
        let hd = self.hd;
        let (nh, nkv) = (l.n_head, l.n_head_kv);
        let inp_sa = cur.clone();

        let mut inputs = [cur.clone(), cur.clone(), cur.clone()];
        if let Some((q, nq)) = &l.attn_res_query {
            let mixed = self.mix(blocks, &cur, q, *nq);
            for j in 0..3 {
                inputs[j] = mixed[if l.swa { 0 } else { j }].clone();
            }
        }
        if let Some(m) = msgs {
            let ctl = l.ctl.as_ref().unwrap();
            for j in 0..3 {
                inputs[j] = self.control(&inputs[j], &m[CTRL_Q + j], ctl, CTRL_Q + j);
            }
        }
        if l.swa {
            let w = l.attn_norm.as_ref().unwrap();
            if msgs.is_some() {
                for inp in inputs.iter_mut() {
                    rmsnorm(inp, Some(w), self.eps);
                }
            } else {
                rmsnorm(&mut inputs[0], Some(w), self.eps);
                inputs[1] = inputs[0].clone();
                inputs[2] = inputs[0].clone();
            }
        } else {
            let w = l.qkv_norm.as_ref().unwrap();
            for j in 0..3 {
                rmsnorm(&mut inputs[j], Some(&w[j]), self.eps);
            }
        }

        let s = &mut st.slots[slot];
        crate::trace("q_in", slot, &inputs[0]);
        let mut q = matvec_new(&l.wq, &inputs[0]);
        let kdyn = l.k_conv_proj.as_ref().map(|p| matvec_new(p, &inputs[1]));
        let vdyn = l.v_conv_proj.as_ref().map(|p| matvec_new(p, &inputs[2]));
        let mut k = self.convolve(matvec_new(&l.wk, &inputs[1]), &l.k_conv, kdyn.as_deref(), &mut s.hist[0]);
        let mut v = self.convolve(matvec_new(&l.wv, &inputs[2]), &l.v_conv, vdyn.as_deref(), &mut s.hist[1]);
        let mut r = matvec_new(&l.rel, &inputs[0]);
        Self::clip_rows(&mut q, hd, Some(&l.q_clip), self.qk_clip);
        Self::clip_rows(&mut k, hd, None, self.qk_clip);
        for vh in v.chunks_mut(hd) {
            rmsnorm(vh, None, self.eps);
        }
        Self::clip_rows(&mut r, self.n_rel, Some(&l.rel_clip), self.rel_clip);
        if !l.swa {
            for x in q.iter_mut() {
                *x *= tau;
            }
            for x in r.iter_mut() {
                *x *= tau;
            }
        }
        crate::trace("Qcur", slot, &q);
        crate::trace("Kcur", slot, &k);
        crate::trace("Vcur", slot, &v);
        crate::trace("Rcur", slot, &r);
        let sinks = matvec_new(&l.sink_proj, &inputs[0]);
        let ext = if l.swa { self.n_swa } else { self.rel_extent };
        s.k.extend_from_slice(&k);
        s.v.extend_from_slice(&v);

        let n_kv = pos + 1;
        let group = nh / nkv;
        let scale = 1.0 / sqrtf(hd as f32);
        let kvd = nkv * hd;
        let mut att = vec![0.0f32; nh * hd];
        let mut sc: Vec<f32> = Vec::with_capacity(n_kv);
        for h in 0..nh {
            let kh = h / group;
            let qh = &q[h * hd..(h + 1) * hd];
            let rh = &r[h * self.n_rel..(h + 1) * self.n_rel];
            sc.clear();
            let mut first = n_kv;
            for j in 0..n_kv {
                let d = pos - j;
                if l.swa && d >= self.n_swa {
                    continue;
                }
                if first == n_kv {
                    first = j;
                }
                let kt = &s.k[j * kvd + kh * hd..j * kvd + (kh + 1) * hd];
                let mut dot = 0.0f32;
                for i in 0..hd {
                    dot += qh[i] * kt[i];
                }
                let bias = if d < ext {
                    let row = &l.rel_bias[d * self.n_rel..(d + 1) * self.n_rel];
                    row.iter().zip(rh).map(|(a, b)| a * b).sum::<f32>()
                } else {
                    0.0
                };
                sc.push(dot * scale + bias - sinks[h]);
            }
            // softmax con un logit sink nullo
            let mut m = 0.0f32;
            for &x in sc.iter() {
                if x > m {
                    m = x;
                }
            }
            let mut sum = libm::expf(0.0 - m) as f64;
            for x in sc.iter_mut() {
                *x = libm::expf(*x - m);
                sum += *x as f64;
            }
            let inv = (1.0 / sum) as f32;
            let o = &mut att[h * hd..(h + 1) * hd];
            for (idx, p) in sc.iter().enumerate() {
                let j = first + idx;
                let vt = &s.v[j * kvd + kh * hd..j * kvd + (kh + 1) * hd];
                for i in 0..hd {
                    o[i] += p * inv * vt[i];
                }
            }
        }
        let gate = matvec_new(&l.wgate, &inputs[0]);
        for (a, g) in att.iter_mut().zip(&gate) {
            *a *= sigmoid(*g);
        }
        crate::trace("kqv_gated", slot, &att);
        let a = self.convolve(matvec_new(&l.wo, &att), &l.post_conv, None, &mut st.slots[slot].hist[2]);
        crate::trace("attn_out", slot, &a);

        let mut cur = if ip % self.block == 0 {
            blocks.push(inp_sa);
            a
        } else {
            let mut c = inp_sa;
            add(&mut c, &a);
            c
        };
        let ffn_inp = cur.clone();
        crate::trace("ffn_inp", slot, &ffn_inp);

        let mut h = self.mix(blocks, &cur, &l.ffn_res_query, 1).pop().unwrap();
        if let Some(m) = msgs {
            h = self.control(&h, &m[CTRL_FFN], l.ctl.as_ref().unwrap(), CTRL_FFN);
        }
        rmsnorm(&mut h, Some(&l.ffn_norm), self.eps);
        let moe = self.moe(l, &h);
        crate::trace("moe_out", slot, &moe);
        let f = self.convolve(moe, &l.ffn_post_conv, None, &mut st.slots[slot].hist[3]);
        crate::trace("ffn_out", slot, &f);
        cur.copy_from_slice(&ffn_inp);
        add(&mut cur, &f);
        crate::trace("l_out", slot, &cur);
        cur
    }

    fn moe(&self, l: &Layer, h: &[f32]) -> Vec<f32> {
        let logits = matvec_new(&l.router, h);
        let probs: Vec<f32> = logits.iter().map(|&x| sigmoid(x)).collect();
        let sel: Vec<f32> = match &l.probs_bias {
            Some(b) => probs.iter().zip(b).map(|(p, b)| p + b).collect(),
            None => probs.clone(),
        };
        let top = super::llama::top_k(&sel, l.n_used);
        let mut w: Vec<f32> = top.iter().map(|&e| probs[e]).collect();
        let sum: f32 = w.iter().sum::<f32>() + 1e-20;
        for x in w.iter_mut() {
            *x /= sum;
            if self.w_scale != 0.0 && self.w_scale != 1.0 {
                *x *= self.w_scale;
            }
        }
        let ff = l.gate_up.ne[1] / 2;
        let mut out = vec![0.0f32; self.e];
        for (&ex, &wt) in top.iter().zip(&w) {
            let gu = matvec_new(&l.gate_up.slice2(ex), h);
            let act: Vec<f32> = (0..ff)
                .map(|i| {
                    let g = gu[i];
                    let u = gu[ff + i];
                    let a = self.situ_beta * tanhf(g / self.situ_beta) * sigmoid(g);
                    let u = if self.situ_lin > 0.0 { self.situ_lin * tanhf(u / self.situ_lin) } else { u };
                    a * u
                })
                .collect();
            let y = matvec_new(&l.down.slice2(ex), &act);
            for (o, v) in out.iter_mut().zip(&y) {
                *o += wt * v;
            }
        }
        out
    }

    pub fn forward(&self, st: &mut Inkling2State, token: u32) -> Vec<f32> {
        let pos = st.pos;
        let tau = 1.0 + self.log_alpha * logf((((pos + 1) as f32) / self.log_floor).max(1.0));
        let mut cur = get_row(&self.embd, token as usize);
        rmsnorm(&mut cur, Some(&self.tok_norm), self.eps);
        let mut blocks: Vec<Vec<f32>> = Vec::new();
        let n = self.layers.len();
        match self.core {
            None => {
                for il in 0..n {
                    cur = self.layer(st, il, il, cur, &mut blocks, None, tau);
                }
            }
            Some((start, end, passes)) => {
                for il in 0..start {
                    cur = self.layer(st, il, il, cur, &mut blocks, None, tau);
                }
                let prelude = cur.clone();
                let prelude_blocks = blocks.clone();
                for pass in 0..passes {
                    let mut bank: Vec<Vec<f32>> = Vec::new();
                    let mut bank_out: Vec<f32> = Vec::new();
                    if pass > 0 {
                        bank = blocks[prelude_blocks.len() + 1..].to_vec();
                        bank_out = cur;
                        cur = prelude.clone();
                        blocks = prelude_blocks.clone();
                    }
                    for ip in start..end {
                        if pass == 0 {
                            cur = self.layer(st, ip, ip, cur, &mut blocks, None, tau);
                        } else {
                            let slot = n + (pass - 1) * (end - start) + (ip - start);
                            let ctl = self.layers[ip].ctl.as_ref().unwrap();
                            let msgs = self.mix(&bank, &bank_out, &ctl.msg_query, 4);
                            cur = self.layer(st, slot, ip, cur, &mut blocks, Some(&msgs), tau);
                        }
                    }
                }
                for il in end..n {
                    cur = self.layer(st, il, il, cur, &mut blocks, None, tau);
                }
            }
        }
        let mut y = self.mix(&blocks, &cur, &self.out_res_query, 1).pop().unwrap();
        rmsnorm(&mut y, Some(&self.out_norm), self.eps);
        crate::trace("result_norm", 0, &y);
        st.pos += 1;
        let mut logits = matvec_new(&self.output, &y);
        if self.softcap != 0.0 {
            for x in logits.iter_mut() {
                *x = self.softcap * tanhf(*x / self.softcap);
            }
        }
        logits.truncate(self.n_vocab);
        logits
    }
}
