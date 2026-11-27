//! Il corpo: cosa il kernel sa della scheda su cui si è svegliato e cosa propone per sfruttarla.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::interp::Sys;
use crate::plat;

pub struct Body {
    pub rev: u32,
    pub serial: u64,
    pub total_mem: u64,
    pub arm_mem: (u32, u32),
    pub fb: (usize, usize),
    pub dt_model: String,
    pub model_bytes: usize,
}

pub static mut BODY: Option<Body> = None;

pub fn body() -> &'static Body {
    unsafe { (*core::ptr::addr_of!(BODY)).as_ref().unwrap() }
}

pub fn features() -> (bool, bool, bool) {
    let isar0 = plat::sysreg_isar0();
    let dotprod = (isar0 >> 44) & 0xf >= 1;
    let lse = (isar0 >> 20) & 0xf >= 2;
    let crc = (isar0 >> 16) & 0xf >= 1;
    (dotprod, lse, crc)
}

fn gb(b: u64) -> String {
    if b >= 1 << 30 {
        format!("{} GB", b >> 30)
    } else {
        format!("{} MB", b >> 20)
    }
}

/// impronta del corpo: cambia se cambia la scheda (seriale, core, memoria)
pub fn fingerprint() -> u64 {
    let b = body();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for v in [b.serial, plat::midr(), b.total_mem, b.rev as u64] {
        for i in 0..8 {
            h ^= (v >> (i * 8)) & 0xff;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

pub fn sys_lines(s: &Sys) -> Vec<(String, String)> {
    let b = body();
    let mut v = Vec::new();
    let name = if b.dt_model.is_empty() { String::from(plat::model_name(b.rev)) } else { b.dt_model.clone() };
    v.push((String::from("Scheda"), name));
    let clk = plat::arm_clock().map(|c| format!(" a {} MHz", c / 1_000_000)).unwrap_or_default();
    v.push((String::from("CPU"), format!("4 × {}{} (1 attivo)", plat::cpu_name(), clk)));
    v.push((String::from("Memoria"), format!("{} totali, {} al kernel", gb(b.total_mem), gb(b.arm_mem.1 as u64))));
    let (used, tot) = crate::heap::stats();
    v.push((String::from("Heap"), format!("{} MB usati su {} MB", used >> 20, tot >> 20)));
    if let Some(t) = plat::temperature() {
        v.push((String::from("Temperatura"), format!("{:.1} °C", t as f32 / 1000.0)));
    }
    v.push((String::from("Schermo"), format!("{} × {}, 32 bit", b.fb.0, b.fb.1)));
    let (dp, lse, _) = features();
    v.push((String::from("SIMD"), format!("NEON{}{}", if dp { " + dotprod" } else { "" }, if lse { " + LSE" } else { "" })));
    let nm = s.models.list().count();
    v.push((
        String::from("Modelli"),
        if nm == 0 { String::from("nessuno (initramfs vuoto)") } else { format!("{} registrati, {} KB", nm, b.model_bytes >> 10) },
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
    let (dp, _, _) = features();
    r.push_str("\nProposte per sfruttare questo corpo:\n");
    let mut n = 1;
    let mut p = |r: &mut String, s: &str| {
        r.push_str(&format!("{}. {}\n", n, s));
        n += 1;
    };
    p(&mut r, "Avviare gli altri 3 core (spin-table del firmware) e dividere tra loro le moltiplicazioni di matrici: circa 3,5 volte più token al secondo.");
    if dp {
        p(&mut r, "Il core ha l'istruzione SDOT: usarla per i prodotti scalari Q8_0/Q4_0, oggi calcolati in modo scalare (guadagno atteso 4–8 volte).");
    } else {
        p(&mut r, "Usare NEON per i prodotti scalari Q8_0/Q4_0, oggi calcolati in modo scalare.");
    }
    if b.total_mem > b.arm_mem.1 as u64 + (512 << 20) {
        p(&mut r, &format!("Il kernel usa solo il primo GB: mappare tutti i {} per modelli più grandi.", gb(b.total_mem)));
    }
    if b.total_mem >= 8 << 30 {
        p(&mut r, "Con 8–16 GB ci sta un modello da 1–3 miliardi di parametri in Q4. Inkling 2 Mini (17,6 GB, ~20 GB di lavoro) non entra: resta sul PC.");
    }
    if let Some(t) = plat::temperature() {
        if t > 70_000 {
            p(&mut r, "Temperatura alta: serve il dissipatore attivo prima di addestrare nelle pause.");
        } else {
            p(&mut r, "Temperatura sotto controllo: il sensore c'è già, utile per le «pause vere» della Fase 6.");
        }
    }
    if plat::board() == plat::Board::Pi5 {
        p(&mut r, "Tastiera, mouse, rete e SSD passano dal chip RP1 sul PCIe: è il prossimo driver da scrivere. Per ora si parla via UART di debug.");
    }
    r
}
