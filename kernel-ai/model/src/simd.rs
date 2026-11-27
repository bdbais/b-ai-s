//! Nuclei AVX2 per i prodotti scalari, con scelta a tempo di esecuzione.
//!
//! Il rilevamento usa CPUID e XGETBV direttamente (niente `std`), così funziona anche nel
//! kernel. I prodotti interi (Q8_0, Q4_0, IQ4_NL) danno la stessa somma intera del percorso
//! scalare; il percorso F32 usa moltiplicazioni e somme separate (niente FMA) e la stessa
//! riduzione finale, quindi il risultato è identico bit per bit.

use core::sync::atomic::{AtomicU8, Ordering};

// 0 = non ancora rilevato, 1 = scalare, 2 = AVX2
static MODE: AtomicU8 = AtomicU8::new(0);
static FORCE_OFF: AtomicU8 = AtomicU8::new(0);

/// Disattiva (false) o riattiva (true) i nuclei SIMD; utile per il collaudo.
pub fn set_simd(enabled: bool) {
    FORCE_OFF.store(if enabled { 0 } else { 1 }, Ordering::Relaxed);
}

/// Vero se i prodotti scalari passano dai nuclei AVX2.
#[inline]
pub fn simd_active() -> bool {
    if FORCE_OFF.load(Ordering::Relaxed) != 0 {
        return false;
    }
    match MODE.load(Ordering::Relaxed) {
        2 => true,
        1 => false,
        _ => {
            let ok = detect();
            MODE.store(if ok { 2 } else { 1 }, Ordering::Relaxed);
            ok
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn detect() -> bool {
    use core::arch::x86_64::{__cpuid, __cpuid_count};
    // SAFETY: CPUID esiste su ogni CPU x86_64
    let l1 = __cpuid(1);
    let fma = l1.ecx & (1 << 12) != 0;
    let osxsave = l1.ecx & (1 << 27) != 0;
    let avx = l1.ecx & (1 << 28) != 0;
    if !(fma && osxsave && avx) {
        return false;
    }
    let max = __cpuid(0).eax;
    if max < 7 {
        return false;
    }
    let l7 = __cpuid_count(7, 0);
    let avx2 = l7.ebx & (1 << 5) != 0;
    // il sistema (o il kernel) deve aver abilitato lo stato SSE e AVX in XCR0
    let xcr0 = unsafe { xgetbv0() };
    avx2 && (xcr0 & 0b110) == 0b110
}

#[cfg(not(target_arch = "x86_64"))]
fn detect() -> bool {
    false
}

#[cfg(target_arch = "x86_64")]
unsafe fn xgetbv0() -> u64 {
    let lo: u32;
    let hi: u32;
    // SAFETY: chiamata solo con OSXSAVE attivo (CPUID.1:ECX[27])
    core::arch::asm!("xgetbv", in("ecx") 0u32, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags));
    ((hi as u64) << 32) | lo as u64
}

#[cfg(target_arch = "x86_64")]
pub mod avx2 {
    use core::arch::x86_64::*;

    use crate::quant::IQ4NL_VALUES;

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn hsum_i32(v: __m256i) -> i32 {
        let lo = _mm256_castsi256_si128(v);
        let hi = _mm256_extracti128_si256(v, 1);
        let s = _mm_add_epi32(lo, hi);
        let s = _mm_add_epi32(s, _mm_shuffle_epi32(s, 0b01_00_11_10));
        let s = _mm_add_epi32(s, _mm_shuffle_epi32(s, 0b10_11_00_01));
        _mm_cvtsi128_si32(s)
    }

    /// Somma di 32 prodotti i8 × i8 (con |a| ≤ 127, garantito dalla quantizzazione Q8_0).
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn dot_i8(w: __m256i, a: __m256i) -> __m256i {
        let aw = _mm256_sign_epi8(w, w); // |w| come u8 (anche -128 → 128)
        let sa = _mm256_sign_epi8(a, w); // a col segno di w
        let p16 = _mm256_maddubs_epi16(aw, sa);
        _mm256_madd_epi16(p16, _mm256_set1_epi16(1))
    }

    /// Somma intera di un blocco Q8_0 (34 byte) con 32 attivazioni i8.
    #[inline]
    #[target_feature(enable = "avx2")]
    pub unsafe fn isum_q8_0(blk: &[u8], qa: &[i8]) -> i32 {
        debug_assert!(blk.len() >= 34 && qa.len() >= 32);
        let w = _mm256_loadu_si256(blk.as_ptr().add(2) as *const __m256i);
        let a = _mm256_loadu_si256(qa.as_ptr() as *const __m256i);
        hsum_i32(dot_i8(w, a))
    }

    /// Separa i 16 byte di nibble: metà bassa del registro = nibble bassi, metà alta = alti.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn nibbles(blk: &[u8]) -> __m256i {
        let x = _mm_loadu_si128(blk.as_ptr().add(2) as *const __m128i);
        let hi = _mm_srli_epi16(x, 4);
        let v = _mm256_set_m128i(hi, x);
        _mm256_and_si256(v, _mm256_set1_epi8(0x0f))
    }

    /// Somma intera di un blocco Q4_0 (18 byte): Σ (n − 8)·a = Σ n·a − 8·Σ a.
    #[inline]
    #[target_feature(enable = "avx2")]
    pub unsafe fn isum_q4_0(blk: &[u8], qa: &[i8]) -> i32 {
        debug_assert!(blk.len() >= 18 && qa.len() >= 32);
        let n = nibbles(blk);
        let a = _mm256_loadu_si256(qa.as_ptr() as *const __m256i);
        let ones = _mm256_set1_epi16(1);
        let na = _mm256_madd_epi16(_mm256_maddubs_epi16(n, a), ones);
        let sa = _mm256_madd_epi16(_mm256_maddubs_epi16(_mm256_set1_epi8(1), a), ones);
        hsum_i32(na) - 8 * hsum_i32(sa)
    }

    /// Somma intera di un blocco IQ4_NL (18 byte): tabella dei 16 valori via PSHUFB.
    #[inline]
    #[target_feature(enable = "avx2")]
    pub unsafe fn isum_iq4nl(blk: &[u8], qa: &[i8]) -> i32 {
        debug_assert!(blk.len() >= 18 && qa.len() >= 32);
        let t = _mm_loadu_si128(IQ4NL_VALUES.as_ptr() as *const __m128i);
        let tab = _mm256_set_m128i(t, t);
        let w = _mm256_shuffle_epi8(tab, nibbles(blk));
        let a = _mm256_loadu_si256(qa.as_ptr() as *const __m256i);
        hsum_i32(dot_i8(w, a))
    }

    /// Vettore di 8 lane i32 per un blocco: la sua somma orizzontale è la somma intera del blocco.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn block_vec<const K: u8>(blk: *const u8, qa: *const i8) -> __m256i {
        let a = _mm256_loadu_si256(qa as *const __m256i);
        let ones = _mm256_set1_epi16(1);
        match K {
            0 => dot_i8(_mm256_loadu_si256(blk.add(2) as *const __m256i), a),
            1 => {
                let n = nibbles_p(blk);
                let na = _mm256_madd_epi16(_mm256_maddubs_epi16(n, a), ones);
                let sa = _mm256_madd_epi16(_mm256_maddubs_epi16(_mm256_set1_epi8(1), a), ones);
                _mm256_sub_epi32(na, _mm256_slli_epi32(sa, 3))
            }
            _ => {
                let t = _mm_loadu_si128(IQ4NL_VALUES.as_ptr() as *const __m128i);
                let w = _mm256_shuffle_epi8(_mm256_set_m128i(t, t), nibbles_p(blk));
                dot_i8(w, a)
            }
        }
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn nibbles_p(blk: *const u8) -> __m256i {
        let x = _mm_loadu_si128(blk.add(2) as *const __m128i);
        let v = _mm256_set_m128i(_mm_srli_epi16(x, 4), x);
        _mm256_and_si256(v, _mm256_set1_epi8(0x0f))
    }

    /// Riduce 8 vettori (uno per blocco) in un solo vettore con la somma di ciascun blocco.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn reduce8(v: &[__m256i; 8]) -> __m256i {
        let a = _mm256_hadd_epi32(v[0], v[1]);
        let b = _mm256_hadd_epi32(v[2], v[3]);
        let c = _mm256_hadd_epi32(v[4], v[5]);
        let d = _mm256_hadd_epi32(v[6], v[7]);
        let ab = _mm256_hadd_epi32(a, b); // lane: b0 b1 b2 b3 | b0' b1' b2' b3' (metà alta)
        let cd = _mm256_hadd_epi32(c, d);
        let lo = _mm256_permute2x128_si256(ab, cd, 0x20);
        let hi = _mm256_permute2x128_si256(ab, cd, 0x31);
        _mm256_add_epi32(lo, hi) // blocchi 0..3 nella metà bassa, 4..7 nella alta
    }

    /// Prodotto di una riga a blocchi da 32 (K = 0 Q8_0, 1 Q4_0, 2 IQ4_NL) con l'attivazione Q8_0.
    /// Le somme intere si calcolano 8 blocchi alla volta; la somma in virgola mobile procede
    /// blocco per blocco nello stesso ordine e con gli stessi arrotondamenti del percorso scalare.
    #[inline]
    #[target_feature(enable = "avx2")]
    pub unsafe fn dot_blocks<const BS: usize, const K: u8>(row: &[u8], d: &[f32], q: &[i8]) -> f32 {
        let nb = d.len();
        assert!(row.len() >= nb * BS && q.len() >= nb * 32);
        let rp = row.as_ptr();
        let qp = q.as_ptr();
        let scale = |b: usize| crate::quant::f16_to_f32(u16::from_le_bytes([*rp.add(b * BS), *rp.add(b * BS + 1)]));
        let mut s = 0.0f32;
        let mut b = 0;
        let mut vs = [_mm256_setzero_si256(); 8];
        let mut sums = [0i32; 8];
        while b + 8 <= nb {
            for k in 0..8 {
                vs[k] = block_vec::<K>(rp.add((b + k) * BS), qp.add((b + k) * 32));
            }
            _mm256_storeu_si256(sums.as_mut_ptr() as *mut __m256i, reduce8(&vs));
            for k in 0..8 {
                s += scale(b + k) * *d.get_unchecked(b + k) * sums[k] as f32;
            }
            b += 8;
        }
        while b < nb {
            let isum = hsum_i32(block_vec::<K>(rp.add(b * BS), qp.add(b * 32)));
            s += scale(b) * *d.get_unchecked(b) * isum as f32;
            b += 1;
        }
        s
    }

    /// Prodotto F32 con 8 accumulatori, identico al percorso scalare (mul + add, niente FMA).
    #[target_feature(enable = "avx2")]
    pub unsafe fn dot_f32(row: &[u8], x: &[f32]) -> f32 {
        let n = x.len();
        debug_assert!(row.len() >= 4 * n);
        let mut acc = _mm256_setzero_ps();
        let mut i = 0;
        while i + 8 <= n {
            let w = _mm256_loadu_ps(row.as_ptr().add(4 * i) as *const f32);
            let v = _mm256_loadu_ps(x.as_ptr().add(i));
            acc = _mm256_add_ps(acc, _mm256_mul_ps(w, v));
            i += 8;
        }
        let mut lanes = [0.0f32; 8];
        _mm256_storeu_ps(lanes.as_mut_ptr(), acc);
        let mut s: f32 = lanes.iter().sum();
        while i < n {
            let w = f32::from_le_bytes([row[4 * i], row[4 * i + 1], row[4 * i + 2], row[4 * i + 3]]);
            s += w * x[i];
            i += 1;
        }
        s
    }
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    extern crate std;
    use super::*;
    use crate::gguf::DType;
    use crate::quant::{dot, dot_scalar, prepare};
    use alloc::vec::Vec;

    fn rng(seed: &mut u32) -> u32 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 17;
        *seed ^= *seed << 5;
        *seed
    }

    #[test]
    fn simd_uguale_allo_scalare() {
        if !simd_active() {
            std::eprintln!("AVX2 assente: test saltato");
            return;
        }
        let mut seed = 42u32;
        for &n in &[32usize, 64, 96, 256, 288, 4096] {
            let x: Vec<f32> = (0..n).map(|_| (rng(&mut seed) % 20001) as f32 / 1000.0 - 10.0).collect();
            for (dt, bs, be) in [(DType::Q8_0, 34usize, 32usize), (DType::Q4_0, 18, 32), (DType::IQ4NL, 18, 32), (DType::F32, 4, 1)] {
                for _ in 0..20 {
                    let mut w: Vec<u8> = (0..n / be * bs).map(|_| rng(&mut seed) as u8).collect();
                    if dt == DType::F32 {
                        for c in w.chunks_mut(4) {
                            c.copy_from_slice(&((rng(&mut seed) % 2001) as f32 / 100.0 - 10.0).to_le_bytes());
                        }
                    } else {
                        for b in (0..w.len()).step_by(bs) {
                            w[b + 1] &= 0x3b; // scala finita
                        }
                    }
                    let a = prepare(dt, &x);
                    let r = dot_scalar(dt, &w, &a);
                    let v = dot(dt, &w, &a);
                    assert_eq!(r.to_bits(), v.to_bits(), "{:?} n={} scalare {} avx2 {}", dt, n, r, v);
                }
            }
        }
    }
}
