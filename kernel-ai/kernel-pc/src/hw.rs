//! Il corpo: cosa il kernel sa del PC su cui si è svegliato e cosa propone per sfruttarlo.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::interp::Sys;
use crate::plat;

pub struct Body {
    pub cpu: String,
    pub vendor: String,
    pub signature: u32,
    pub threads: u32,
    pub mhz: u64,
    pub ram_total: u64,
    pub heap_bytes: usize,
    pub fb: (usize, usize),
    pub system: String,
    pub uuid: [u8; 16],
    pub loader: String,
    pub model_bytes: usize,
    pub emulated: bool,
}

pub static mut BODY: Option<Body> = None;

pub fn body() -> &'static Body {
    unsafe { (*core::ptr::addr_of!(BODY)).as_ref().unwrap() }
}

fn gb(b: u64) -> String {
    if b >= 1 << 30 {
        format!("{:.1} GB", b as f64 / (1u64 << 30) as f64)
    } else {
        format!("{} MB", b >> 20)
    }
}

/// impronta del corpo: cambia se cambia la macchina (UUID SMBIOS, CPU, memoria, schermo)
pub fn fingerprint() -> u64 {
    let b = body();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for &x in bytes {
            h ^= x as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    };
    eat(&b.uuid);
    eat(b.cpu.as_bytes());
    eat(&b.signature.to_le_bytes());
    eat(&b.threads.to_le_bytes());
    eat(&(b.ram_total >> 20).to_le_bytes());
    eat(&(b.fb.0 as u32).to_le_bytes());
    eat(&(b.fb.1 as u32).to_le_bytes());
    h
}

pub fn machine_name() -> String {
    let b = body();
    if !b.system.is_empty() {
        b.system.clone()
    } else if b.emulated {
        String::from("PC virtuale")
    } else {
        String::from("PC x86_64")
    }
}

pub fn sys_lines(s: &Sys) -> Vec<(String, String)> {
    let b = body();
    let mut v = Vec::new();
    v.push((String::from("Macchina"), machine_name()));
    v.push((String::from("CPU"), format!("{} · {} thread (1 attivo) · ~{} MHz", b.cpu, b.threads, b.mhz)));
    v.push((String::from("Memoria"), format!("{} totali, {} di heap", gb(b.ram_total), gb(b.heap_bytes as u64))));
    let (used, tot) = crate::heap::stats();
    v.push((String::from("Heap"), format!("{} MB usati su {} MB", used >> 20, tot >> 20)));
    v.push((String::from("Schermo"), format!("{} × {}, 32 bit", b.fb.0, b.fb.1)));
    let f = plat::features();
    let mut simd = String::from("SSE2");
    if f.sse42 {
        simd.push_str(" + SSE4.2");
    }
    if f.avx {
        simd.push_str(" + AVX");
    }
    if f.avx2 {
        simd.push_str(" + AVX2");
    }
    if f.fma {
        simd.push_str(" + FMA");
    }
    if f.avx512 {
        simd.push_str(" + AVX-512");
    }
    v.push((String::from("SIMD"), simd));
    let nm = s.models.list().count();
    v.push((
        String::from("Modelli"),
        if nm == 0 { String::from("nessuno (nessun modulo GRUB)") } else { format!("{} registrati, {} KB", nm, b.model_bytes >> 10) },
    ));
    if let Some(t) = s.last_tps {
        v.push((String::from("Ultima gen."), format!("{:.1} token/s", t)));
    }
    v.push((String::from("Impronta"), format!("{:016x}", fingerprint())));
    v
}

pub fn report(s: &Sys) -> String {
    let b = body();
    let mut r = String::new();
    for (k, v) in sys_lines(s) {
        r.push_str(&format!("{:<12} {}\n", k, v));
    }
    if !b.loader.is_empty() {
        r.push_str(&format!("{:<12} {}\n", "Avvio", b.loader));
    }
    let f = plat::features();
    r.push_str("\nProposte per sfruttare questo corpo:\n");
    let mut n = 1;
    let mut p = |r: &mut String, s: &str| {
        r.push_str(&format!("{}. {}\n", n, s));
        n += 1;
    };
    if b.threads > 1 {
        p(&mut r, &format!("Avviare gli altri {} thread (APIC, Fase 4) e dividere le moltiplicazioni di matrici tra i core.", b.threads - 1));
    }
    if f.avx2 && f.fma {
        if bais_model::simd::simd_active() {
            p(&mut r, "I nuclei AVX2 del motore sono attivi. Prossimo passo: la scala f16 con F16C.");
        } else {
            p(&mut r, "La CPU ha AVX2 + FMA ma il motore non li usa: controllare CR4.OSXSAVE e XCR0 all'avvio.");
        }
    } else {
        p(&mut r, "Niente AVX2: i prodotti scalari restano su SSE2.");
    }
    if b.ram_total > (b.heap_bytes as u64) + (512 << 20) {
        p(&mut r, &format!("Il kernel mappa solo i primi 4 GB: mappare tutti i {} per i modelli grandi.", gb(b.ram_total)));
    }
    if b.ram_total >= 24 << 30 {
        p(&mut r, "Con questa RAM Inkling 2 Mini (17,6 GB) ci sta: serve il disco (exFAT) per leggerlo, tappa della Fase 4.");
    }
    if b.emulated {
        p(&mut r, "Sono in una macchina virtuale: niente sensori di temperatura né GPU vera. Sul PC reale serve l'avvio UEFI.");
    } else {
        p(&mut r, "Leggere la temperatura dei core (MSR) per riconoscere le «pause vere» della Fase 6.");
    }
    p(&mut r, "Disco e chiavetta (AHCI/NVMe/USB) per tenere diario, abitudini e tema dopo lo spegnimento.");
    r
}
