//! Formati quantizzati e prodotti scalari. Gli schemi di calcolo seguono quelli di ggml
//! (attivazioni quantizzate in Q8_0 per i pesi Q8_0/Q4_0/IQ4_NL) così i risultati coincidono
//! con llama.cpp entro l'arrotondamento.

use alloc::vec::Vec;

use crate::gguf::{DType, Tensor};

pub fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1f) as u32;
    let mant = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if mant == 0 {
            sign << 31
        } else {
            // subnormale
            let mut e = 127 - 15 + 1;
            let mut m = mant;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            (sign << 31) | ((e as u32) << 23) | ((m & 0x3ff) << 13)
        }
    } else if exp == 31 {
        (sign << 31) | 0x7f80_0000 | (mant << 13)
    } else {
        (sign << 31) | ((exp + 127 - 15) << 23) | (mant << 13)
    };
    f32::from_bits(bits)
}

pub fn f32_to_f16(f: f32) -> u16 {
    // arrotondamento al pari più vicino, come ggml (versione software)
    let x = f.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let mut exp = ((x >> 23) & 0xff) as i32;
    let mut mant = x & 0x7f_ffff;
    if exp == 255 {
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    exp = exp - 127 + 15;
    if exp >= 31 {
        return sign | 0x7c00;
    }
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        mant |= 0x80_0000;
        let shift = (14 - exp) as u32;
        let half = 1u32 << (shift - 1);
        let rest = mant & ((1 << shift) - 1);
        let mut m = mant >> shift;
        if rest > half || (rest == half && (m & 1) == 1) {
            m += 1;
        }
        return sign | m as u16;
    }
    let rest = mant & 0x1fff;
    let mut m = mant >> 13;
    let mut e = exp as u32;
    if rest > 0x1000 || (rest == 0x1000 && (m & 1) == 1) {
        m += 1;
        if m == 0x400 {
            m = 0;
            e += 1;
            if e >= 31 {
                return sign | 0x7c00;
            }
        }
    }
    sign | ((e as u16) << 10) | m as u16
}

#[inline]
fn rd16(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

pub const IQ4NL_VALUES: [i8; 16] = [-127, -104, -83, -65, -49, -35, -22, -10, 1, 13, 25, 38, 53, 69, 89, 113];

/// Dequantizza una riga di `n` elementi.
pub fn dequant_row(dt: DType, src: &[u8], out: &mut [f32]) {
    let n = out.len();
    match dt {
        DType::F32 => {
            for i in 0..n {
                out[i] = f32::from_le_bytes([src[4 * i], src[4 * i + 1], src[4 * i + 2], src[4 * i + 3]]);
            }
        }
        DType::F16 => {
            for i in 0..n {
                out[i] = f16_to_f32(rd16(src, 2 * i));
            }
        }
        DType::Q8_0 => {
            for b in 0..n / 32 {
                let blk = &src[b * 34..b * 34 + 34];
                let d = f16_to_f32(rd16(blk, 0));
                for j in 0..32 {
                    out[b * 32 + j] = d * (blk[2 + j] as i8) as f32;
                }
            }
        }
        DType::Q4_0 => {
            for b in 0..n / 32 {
                let blk = &src[b * 18..b * 18 + 18];
                let d = f16_to_f32(rd16(blk, 0));
                for j in 0..16 {
                    let q = blk[2 + j];
                    out[b * 32 + j] = d * ((q & 0xf) as i32 - 8) as f32;
                    out[b * 32 + j + 16] = d * ((q >> 4) as i32 - 8) as f32;
                }
            }
        }
        DType::IQ4NL => {
            for b in 0..n / 32 {
                let blk = &src[b * 18..b * 18 + 18];
                let d = f16_to_f32(rd16(blk, 0));
                for j in 0..16 {
                    let q = blk[2 + j];
                    out[b * 32 + j] = d * IQ4NL_VALUES[(q & 0xf) as usize] as f32;
                    out[b * 32 + j + 16] = d * IQ4NL_VALUES[(q >> 4) as usize] as f32;
                }
            }
        }
        DType::Other(_) => out.fill(0.0),
    }
}

/// Attivazione preparata per il prodotto scalare con un certo tipo di peso.
pub enum Act {
    F32(Vec<f32>),
    F16(Vec<f32>), // valori già arrotondati a f16, come fa ggml prima del prodotto
    Q8 { d: Vec<f32>, q: Vec<i8>, s: Vec<f32> },
}

pub fn quantize_q8_0(x: &[f32]) -> Act {
    let nb = x.len() / 32;
    let mut d = Vec::with_capacity(nb);
    let mut q = Vec::with_capacity(x.len());
    let mut s = Vec::with_capacity(nb);
    for b in 0..nb {
        let blk = &x[b * 32..b * 32 + 32];
        let mut amax = 0.0f32;
        for &v in blk {
            let a = if v < 0.0 { -v } else { v };
            if a > amax {
                amax = a;
            }
        }
        // come il percorso AVX di ggml: id = 127/amax, arrotondamento al pari più vicino
        let dd = amax / 127.0;
        let id = if amax != 0.0 { 127.0 / amax } else { 0.0 };
        let dh = f16_to_f32(f32_to_f16(dd));
        let mut sum = 0i32;
        for &v in blk {
            let qi = libm::rintf(v * id) as i32;
            let qi = qi.clamp(-128, 127) as i8;
            sum += qi as i32;
            q.push(qi);
        }
        d.push(dh);
        s.push(dh * sum as f32);
    }
    Act::Q8 { d, q, s }
}

pub fn prepare(dt: DType, x: &[f32]) -> Act {
    match dt {
        DType::Q8_0 | DType::Q4_0 | DType::IQ4NL => quantize_q8_0(x),
        DType::F16 => Act::F16(x.iter().map(|&v| f16_to_f32(f32_to_f16(v))).collect()),
        _ => Act::F32(x.into()),
    }
}

/// Prodotto scalare tra una riga di pesi e l'attivazione preparata.
pub fn dot(dt: DType, row: &[u8], a: &Act) -> f32 {
    #[cfg(target_arch = "x86_64")]
    if crate::simd::simd_active() {
        // SAFETY: AVX2 rilevato a tempo di esecuzione (CPUID + XCR0)
        if let Some(v) = unsafe { dot_avx2(dt, row, a) } {
            return v;
        }
    }
    dot_scalar(dt, row, a)
}

/// Percorso AVX2: stesse somme intere e stesso ordine delle somme in virgola mobile dello scalare.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dot_avx2(dt: DType, row: &[u8], a: &Act) -> Option<f32> {
    use crate::simd::avx2;
    match (dt, a) {
        (DType::F32, Act::F32(x)) => Some(avx2::dot_f32(row, x)),
        (DType::Q8_0, Act::Q8 { d, q, .. }) => Some(avx2::dot_blocks::<34, 0>(row, d, q)),
        (DType::Q4_0, Act::Q8 { d, q, .. }) => Some(avx2::dot_blocks::<18, 1>(row, d, q)),
        (DType::IQ4NL, Act::Q8 { d, q, .. }) => Some(avx2::dot_blocks::<18, 2>(row, d, q)),
        _ => None,
    }
}

/// Percorso scalare di riferimento (coincide con llama.cpp entro l'arrotondamento).
pub fn dot_scalar(dt: DType, row: &[u8], a: &Act) -> f32 {
    match (dt, a) {
        (DType::F32, Act::F32(x)) => {
            let mut acc = [0.0f32; 8];
            let n = x.len();
            let mut i = 0;
            while i + 8 <= n {
                for k in 0..8 {
                    let w = f32::from_le_bytes([row[4 * (i + k)], row[4 * (i + k) + 1], row[4 * (i + k) + 2], row[4 * (i + k) + 3]]);
                    acc[k] += w * x[i + k];
                }
                i += 8;
            }
            let mut s: f32 = acc.iter().sum();
            while i < n {
                let w = f32::from_le_bytes([row[4 * i], row[4 * i + 1], row[4 * i + 2], row[4 * i + 3]]);
                s += w * x[i];
                i += 1;
            }
            s
        }
        (DType::F16, Act::F16(x)) => {
            let mut s = 0.0f32;
            for i in 0..x.len() {
                s += f16_to_f32(rd16(row, 2 * i)) * x[i];
            }
            s
        }
        (DType::Q8_0, Act::Q8 { d, q, .. }) => {
            let mut s = 0.0f32;
            for b in 0..d.len() {
                let blk = &row[b * 34..b * 34 + 34];
                let dw = f16_to_f32(rd16(blk, 0));
                let mut isum = 0i32;
                let qa = &q[b * 32..b * 32 + 32];
                for j in 0..32 {
                    isum += (blk[2 + j] as i8 as i32) * (qa[j] as i32);
                }
                s += dw * d[b] * isum as f32;
            }
            s
        }
        (DType::Q4_0, Act::Q8 { d, q, .. }) => {
            let mut s = 0.0f32;
            for b in 0..d.len() {
                let blk = &row[b * 18..b * 18 + 18];
                let dw = f16_to_f32(rd16(blk, 0));
                let qa = &q[b * 32..b * 32 + 32];
                let mut isum = 0i32;
                for j in 0..16 {
                    let v = blk[2 + j];
                    isum += ((v & 0xf) as i32 - 8) * qa[j] as i32 + ((v >> 4) as i32 - 8) * qa[j + 16] as i32;
                }
                s += dw * d[b] * isum as f32;
            }
            s
        }
        (DType::IQ4NL, Act::Q8 { d, q, .. }) => {
            let mut s = 0.0f32;
            for b in 0..d.len() {
                let blk = &row[b * 18..b * 18 + 18];
                let dw = f16_to_f32(rd16(blk, 0));
                let qa = &q[b * 32..b * 32 + 32];
                let mut isum = 0i32;
                for j in 0..16 {
                    let v = blk[2 + j];
                    isum += IQ4NL_VALUES[(v & 0xf) as usize] as i32 * qa[j] as i32
                        + IQ4NL_VALUES[(v >> 4) as usize] as i32 * qa[j + 16] as i32;
                }
                s += dw * d[b] * isum as f32;
            }
            s
        }
        _ => 0.0,
    }
}

/// y = W x, con W di forma ggml [n_in, n_out].
pub fn matvec(w: &Tensor, x: &[f32], y: &mut [f32]) {
    debug_assert_eq!(x.len(), w.ne[0]);
    let a = prepare(w.dtype, x);
    for (r, out) in y.iter_mut().enumerate().take(w.ne[1]) {
        *out = dot(w.dtype, w.row(r), &a);
    }
}

pub fn matvec_new(w: &Tensor, x: &[f32]) -> Vec<f32> {
    let mut y = alloc::vec![0.0; w.ne[1]];
    matvec(w, x, &mut y);
    y
}

/// Riga `r` di un tensore, dequantizzata (get_rows).
pub fn get_row(w: &Tensor, r: usize) -> Vec<f32> {
    let mut out = alloc::vec![0.0; w.ne[0]];
    dequant_row(w.dtype, w.row(r), &mut out);
    out
}

/// Tensore piccolo (norme, query) letto tutto in f32.
pub fn to_f32(w: &Tensor) -> Vec<f32> {
    let n = w.n_elements();
    let mut out = alloc::vec![0.0; n];
    let rows = w.n_rows();
    let ne0 = w.ne[0];
    for r in 0..rows {
        dequant_row(w.dtype, w.row(r), &mut out[r * ne0..(r + 1) * ne0]);
    }
    out
}
