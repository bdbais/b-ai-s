//! Ponte tra l'applicazione UEFI in C (gnu-efi) e bais-storage.
//!
//! Il C apre il protocollo BlockIo del firmware e passa una funzione di
//! lettura; qui si cerca il volume exFAT, si elenca la radice, si legge per
//! intero un file calcolandone lo sha256 e lo si confronta con quello atteso,
//! scritto in `<file>.sha256` accanto al file (formato di `sha256sum`).
#![no_std]

use bais_storage::exfat::Volume;
use bais_storage::sha256::{to_hex, Sha256};
use bais_storage::{find_exfat, BlockDevice, Error, Partition, Result};
use core::ffi::c_void;

#[repr(C)]
pub struct CDev {
    pub ctx: *mut c_void,
    pub block_size: u32,
    pub num_blocks: u64,
    /// Legge `nbytes` (multiplo di block_size) dal blocco `lba`; 0 = ok.
    pub read: extern "C" fn(ctx: *mut c_void, lba: u64, nbytes: u64, buf: *mut u8) -> i32,
    /// Scrive una riga di testo UTF-8 sulla console.
    pub log: extern "C" fn(ctx: *mut c_void, text: *const u8, len: usize),
    /// Contatore del tempo (TSC) e sua frequenza, per misurare la velocità.
    pub ticks: extern "C" fn() -> u64,
    pub ticks_per_sec: u64,
}

#[repr(C)]
pub struct Report {
    pub error: i32,
    pub part_start: u64,
    pub part_blocks: u64,
    pub file_size: u64,
    pub bytes_read: u64,
    pub read_requests: u64,
    pub elapsed_ms: u64,
    pub sha256_hex: [u8; 64],
    /// 1 = uguale all'atteso, 0 = diverso, -1 = file .sha256 assente.
    pub verified: i32,
}

struct Dev<'a> {
    c: &'a CDev,
    requests: u64,
}

impl BlockDevice for Dev<'_> {
    fn block_size(&self) -> u32 {
        self.c.block_size
    }
    fn num_blocks(&self) -> u64 {
        self.c.num_blocks
    }
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<()> {
        self.requests += 1;
        if (self.c.read)(self.c.ctx, lba, buf.len() as u64, buf.as_mut_ptr()) == 0 {
            Ok(())
        } else {
            Err(Error::Io)
        }
    }
}

/// Piccolo formattatore senza allocazioni per le righe di log.
struct Line {
    b: [u8; 512],
    n: usize,
}

impl core::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let k = s.len().min(self.b.len() - self.n);
        self.b[self.n..self.n + k].copy_from_slice(&s.as_bytes()[..k]);
        self.n += k;
        Ok(())
    }
}

macro_rules! log {
    ($dev:expr, $($t:tt)*) => {{
        let mut l = Line { b: [0; 512], n: 0 };
        let _ = core::fmt::write(&mut l, format_args!($($t)*));
        ($dev.log)($dev.ctx, l.b.as_ptr(), l.n);
    }};
}

fn run(c: &CDev, path: &str, buf: &mut [u8], r: &mut Report) -> Result<()> {
    let mut dev = Dev { c, requests: 0 };
    let (start, len) = find_exfat(&mut dev)?;
    r.part_start = start;
    r.part_blocks = len;
    let mut v = Volume::open(Partition::new(dev, start, len))?;
    let i = v.info();
    let mut lab = [0u8; 64];
    let mut ln = 0;
    for ch in char::decode_utf16(i.label[..i.label_len as usize].iter().copied()) {
        let ch = ch.unwrap_or('?');
        if ln + ch.len_utf8() <= lab.len() {
            ch.encode_utf8(&mut lab[ln..]);
            ln += ch.len_utf8();
        }
    }
    log!(
        c,
        "exFAT «{}»: dal blocco {}, {} MB, cluster da {} KB",
        core::str::from_utf8(&lab[..ln]).unwrap_or("?"),
        start,
        len * c.block_size as u64 / 1_000_000,
        i.cluster_bytes / 1024
    );

    let mut root = v.root()?;
    log!(c, "radice:");
    while let Some(e) = v.read_dir(&mut root)? {
        let mut nb = [0u8; 765];
        let name = e.name_utf8(&mut nb)?;
        if e.is_dir() {
            log!(c, "  [cartella] {}", name);
        } else {
            log!(c, "  {:>14} {}", e.data_length, name);
        }
    }

    // Se il percorso predefinito non c'è, prende il .gguf più grande in /modelli.
    let mut chosen = [0u8; 300];
    let wanted = path;
    let mut path = path;
    if let Err(Error::NotFound) = v.lookup(path) {
        if let Ok(mut d) = v.open_dir_path("/modelli") {
            let mut best = 0u64;
            let mut best_len = 0usize;
            while let Some(e) = v.read_dir(&mut d)? {
                let mut nb = [0u8; 765];
                let name = e.name_utf8(&mut nb)?;
                let is_gguf = name.len() > 5 && name[name.len() - 5..].eq_ignore_ascii_case(".gguf");
                if !e.is_dir() && is_gguf && e.data_length > best && name.len() + 9 <= chosen.len() {
                    best = e.data_length;
                    chosen[..9].copy_from_slice(b"/modelli/");
                    chosen[9..9 + name.len()].copy_from_slice(name.as_bytes());
                    best_len = 9 + name.len();
                }
            }
            if best_len > 0 {
                path = core::str::from_utf8(&chosen[..best_len]).map_err(|_| Error::NotFound)?;
                log!(c, "{} non c'è: uso il modello più grande, {}", wanted, path);
            }
        }
    }

    // sha256 atteso, se c'è «<percorso>.sha256»
    let mut expected = [0u8; 64];
    let mut have_expected = false;
    {
        let mut p = [0u8; 300];
        let pl = path.len();
        if pl + 7 <= p.len() {
            p[..pl].copy_from_slice(path.as_bytes());
            p[pl..pl + 7].copy_from_slice(b".sha256");
            if let Ok(sp) = core::str::from_utf8(&p[..pl + 7]) {
                if let Ok(mut f) = v.open_path(sp) {
                    if v.read(&mut f, &mut expected)? == 64 {
                        for x in expected.iter_mut() {
                            x.make_ascii_lowercase();
                        }
                        have_expected = true;
                    }
                }
            }
        }
    }

    let mut f = v.open_path(path)?;
    r.file_size = f.len();
    log!(c, "lettura di {} ({} byte)...", path, f.len());
    let mut h = Sha256::new();
    let t0 = (c.ticks)();
    let mut next_report = 1u64 << 30;
    loop {
        let n = v.read(&mut f, buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        r.bytes_read += n as u64;
        if r.bytes_read >= next_report {
            let ms = ((c.ticks)() - t0) / (c.ticks_per_sec / 1000).max(1);
            log!(c, "  {} GiB letti, {} s", r.bytes_read >> 30, ms / 1000);
            next_report += 1 << 30;
        }
    }
    r.elapsed_ms = ((c.ticks)() - t0) / (c.ticks_per_sec / 1000).max(1);
    to_hex(&h.finish(), &mut r.sha256_hex);
    r.verified = if have_expected {
        (r.sha256_hex == expected) as i32
    } else {
        -1
    };
    r.read_requests = v.into_inner().into_inner().requests;
    Ok(())
}

/// Punto d'ingresso chiamato dal C. Restituisce 0 o un codice di `Error`.
///
/// # Safety
/// `dev`, `path` (UTF-8, `path_len` byte), `buf` (`buf_len` byte) e `report`
/// devono essere validi per tutta la chiamata.
#[no_mangle]
pub unsafe extern "C" fn bais_sonda(
    dev: *const CDev,
    path: *const u8,
    path_len: usize,
    buf: *mut u8,
    buf_len: usize,
    report: *mut Report,
) -> i32 {
    let c = &*dev;
    let r = &mut *report;
    *r = Report {
        error: 0,
        part_start: 0,
        part_blocks: 0,
        file_size: 0,
        bytes_read: 0,
        read_requests: 0,
        elapsed_ms: 0,
        sha256_hex: [b'-'; 64],
        verified: -1,
    };
    let path = match core::str::from_utf8(core::slice::from_raw_parts(path, path_len)) {
        Ok(p) => p,
        Err(_) => return Error::NotFound.code(),
    };
    // Il buffer va allineato al blocco per le letture dirette.
    let bs = c.block_size as usize;
    let usable = buf_len / bs * bs;
    let buf = core::slice::from_raw_parts_mut(buf, usable);
    match run(c, path, buf, r) {
        Ok(()) => 0,
        Err(e) => {
            log!(c, "errore: {}", e.as_str());
            r.error = e.code();
            e.code()
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
