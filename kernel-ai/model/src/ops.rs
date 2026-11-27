//! Operazioni elementari sui vettori.

use libm::{expf, sqrtf};

pub fn rmsnorm(x: &mut [f32], w: Option<&[f32]>, eps: f32) {
    let mut ss = 0.0f64;
    for &v in x.iter() {
        ss += (v as f64) * (v as f64);
    }
    let mean = (ss / x.len() as f64) as f32;
    let scale = 1.0 / sqrtf(mean + eps);
    match w {
        Some(w) => {
            for (v, g) in x.iter_mut().zip(w) {
                *v = *v * scale * g;
            }
        }
        None => {
            for v in x.iter_mut() {
                *v *= scale;
            }
        }
    }
}

pub fn softmax(x: &mut [f32]) {
    let mut m = f32::NEG_INFINITY;
    for &v in x.iter() {
        if v > m {
            m = v;
        }
    }
    let mut s = 0.0f64;
    for v in x.iter_mut() {
        *v = expf(*v - m);
        s += *v as f64;
    }
    let inv = (1.0 / s) as f32;
    for v in x.iter_mut() {
        *v *= inv;
    }
}

#[inline]
pub fn silu(x: f32) -> f32 {
    x / (1.0 + expf(-x))
}

#[inline]
pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + expf(-x))
}

pub fn add(a: &mut [f32], b: &[f32]) {
    for (x, y) in a.iter_mut().zip(b) {
        *x += *y;
    }
}

pub fn argmax(x: &[f32]) -> usize {
    let mut best = 0;
    for i in 1..x.len() {
        if x[i] > x[best] {
            best = i;
        }
    }
    best
}
