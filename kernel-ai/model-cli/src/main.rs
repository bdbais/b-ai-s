//! Strumento di collaudo su host per bais-model: stesso motore del kernel.
//! uso: bais-model-cli <modello.gguf> "<prompt>" <n_token>
//! stampa gli id del prompt e dei token generati in modo greedy (formato del riferimento llama.cpp).

use bais_model::manager::{Manager, Task};
use bais_model::sampler::Params;
use std::time::Instant;

static SUMS: std::sync::Mutex<Vec<(String, usize, f64)>> = std::sync::Mutex::new(Vec::new());

fn tr(name: &str, l: usize, v: &[f32]) {
    if name == "first_logits" {
        if let Ok(p) = std::env::var("DUMP") {
            let t: Vec<String> = v.iter().map(|x| format!("{:.7}", x)).collect();
            std::fs::write(p, t.join("\n") + "\n").unwrap();
        }
    }
    if let Ok(w) = std::env::var("DUMPT") {
        if format!("{}-{}", name, l) == w {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/bt.txt").unwrap();
            for x in v {
                writeln!(f, "{:.7}", x).unwrap();
            }
        }
    }
    let s: f64 = v.iter().map(|&x| x as f64).sum();
    let mut g = SUMS.lock().unwrap();
    if let Some(e) = g.iter_mut().find(|e| e.0 == name && e.1 == l) {
        e.2 += s;
    } else {
        g.push((name.to_string(), l, s));
    }
}

fn main() {
    if std::env::var("BAIS_TRACE").is_ok() {
        unsafe { bais_model::TRACE = Some(tr) };
    }
    if std::env::var("BAIS_NOSIMD").is_ok() {
        bais_model::simd::set_simd(false);
    }
    let a: Vec<String> = std::env::args().collect();
    if a.len() == 2 && a[1] == "--bench" {
        bench();
        return;
    }
    if a.len() < 4 {
        eprintln!("uso: {} modello.gguf \"prompt\" n", a[0]);
        std::process::exit(1);
    }
    let buf = std::fs::read(&a[1]).expect("lettura del modello");
    let buf: &'static [u8] = Box::leak(buf.into_boxed_slice());
    let n: usize = a[3].parse().unwrap();
    let mut m = Manager::new(usize::MAX);
    let id = m.register(buf).expect("registrazione");
    m.load(id).expect("caricamento");
    for i in m.list() {
        eprintln!("[{}] {} {} {:.1} MB {} parametri stato {:?}", i.arch, i.name, i.quant, i.bytes as f64 / 1e6, i.n_params, i.status);
    }
    let id = m.choose(Task::Generate).unwrap();
    let ptoks = m.tokenize(id, &a[2]).unwrap();
    print!("prompt:");
    for t in &ptoks {
        print!(" {}", t);
    }
    println!();
    let t0 = Instant::now();
    let out = m.generate(id, &a[2], n, Params::GREEDY, |_, _| true).unwrap();
    let dt = t0.elapsed().as_secs_f64();
    print!("gen:");
    for t in &out.tokens {
        print!(" {}", t);
    }
    println!();
    eprintln!("testo: {:?}", out.text);
    for (n, l, s) in SUMS.lock().unwrap().iter() {
        eprintln!("TRACE {}-{} {:.6}", n, l, s);
    }
    eprintln!("{:.1} token/s", (out.prompt_tokens + out.tokens.len()) as f64 / dt);
}

/// Micro-benchmark del prodotto scalare: 4096 righe × 4096 colonne per formato.
fn bench() {
    use bais_model::gguf::DType;
    use bais_model::quant::{dot, prepare};
    let n = 4096usize;
    let rows = 4096usize;
    let mut seed = 0x1234_5678u32;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let x: Vec<f32> = (0..n).map(|_| (rnd() % 2001) as f32 / 1000.0 - 1.0).collect();
    for (name, dt, bs, be) in [("Q8_0", DType::Q8_0, 34usize, 32usize), ("Q4_0", DType::Q4_0, 18, 32), ("IQ4_NL", DType::IQ4NL, 18, 32), ("F32", DType::F32, 4, 1)] {
        let row_bytes = n / be * bs;
        let mut w: Vec<u8> = (0..row_bytes * rows).map(|_| rnd() as u8).collect();
        if dt != DType::F32 {
            // scala f16 piccola e valida all'inizio di ogni blocco
            for b in (0..w.len()).step_by(bs) {
                w[b] = 0x00;
                w[b + 1] = 0x20;
            }
        } else {
            for c in w.chunks_mut(4) {
                c.copy_from_slice(&((rnd() % 2001) as f32 / 1000.0 - 1.0).to_le_bytes());
            }
        }
        let a = prepare(dt, &x);
        let mut res = [0f64; 2];
        let mut chk = [0f64; 2];
        for (k, on) in [false, true].into_iter().enumerate() {
            bais_model::simd::set_simd(on);
            let t0 = Instant::now();
            let mut s = 0f64;
            for r in 0..rows {
                s += dot(dt, &w[r * row_bytes..(r + 1) * row_bytes], &a) as f64;
            }
            res[k] = t0.elapsed().as_secs_f64();
            chk[k] = s;
        }
        let gop = 2.0 * (n * rows) as f64 / 1e9;
        println!("{:7} scalare {:6.2} GOP/s  AVX2 {:6.2} GOP/s  ×{:.1}  {}", name, gop / res[0], gop / res[1], res[0] / res[1], if chk[0] == chk[1] { "identico" } else { "DIVERSO" });
    }
    bais_model::simd::set_simd(true);
}
