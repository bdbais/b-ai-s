//! Piattaforma: riconoscimento della scheda, MMU, UART PL011, mailbox del firmware, timer.
//!
//! Una sola immagine per tutte le schede: il modello si riconosce dal core (MIDR_EL1)
//! e da lì si ricavano gli indirizzi delle periferiche.

use core::arch::asm;
use core::ptr::{read_volatile, write_volatile};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Board {
    /// Raspberry Pi 5 (BCM2712, Cortex-A76)
    Pi5,
    /// Raspberry Pi 4 / 400 (BCM2711, Cortex-A72)
    Pi4,
    /// Raspberry Pi 3 / Zero 2 W (BCM2837, Cortex-A53), anche QEMU raspi3b
    Pi3,
}

impl Board {
    pub fn name(self) -> &'static str {
        match self {
            Board::Pi5 => "Raspberry Pi 5",
            Board::Pi4 => "Raspberry Pi 4",
            Board::Pi3 => "Raspberry Pi 3",
        }
    }
    fn mbox_base(self) -> usize {
        match self {
            Board::Pi5 => 0x10_7C01_3880,
            Board::Pi4 => 0xFE00_B880,
            Board::Pi3 => 0x3F00_B880,
        }
    }
    fn uart_base(self) -> usize {
        match self {
            Board::Pi5 => 0x10_7D00_1000, // UART di debug (connettore a 3 pin)
            Board::Pi4 => 0xFE20_1000,    // PL011 su GPIO14/15 (dtoverlay=disable-bt)
            Board::Pi3 => 0x3F20_1000,
        }
    }
    fn gpio_base(self) -> Option<usize> {
        match self {
            Board::Pi5 => None, // i GPIO stanno sull'RP1, dietro il PCIe
            Board::Pi4 => Some(0xFE20_0000),
            Board::Pi3 => Some(0x3F20_0000),
        }
    }
    /// alias di bus con cui il VideoCore vede la RAM
    fn bus(self, a: usize) -> u32 {
        match self {
            Board::Pi5 => a as u32,
            _ => (a as u32) | 0xC000_0000,
        }
    }
    /// è memoria di periferica (da mappare come Device)?
    fn is_device(self, a: u64) -> bool {
        match self {
            Board::Pi5 => (0x10_0000_0000..0x20_0000_0000).contains(&a),
            Board::Pi4 => (0xFC00_0000..0x1_0000_0000).contains(&a) || (0x6_0000_0000..0x8_0000_0000).contains(&a),
            Board::Pi3 => (0x3F00_0000..0x4100_0000).contains(&a),
        }
    }
}

pub static mut BOARD: Board = Board::Pi3;

pub fn board() -> Board {
    unsafe { BOARD }
}

pub fn midr() -> u64 {
    let v: u64;
    unsafe { asm!("mrs {}, midr_el1", out(reg) v) };
    v
}

pub fn detect() -> Board {
    let b = match (midr() >> 4) & 0xfff {
        0xD0B => Board::Pi5,
        0xD08 => Board::Pi4,
        _ => Board::Pi3,
    };
    unsafe { BOARD = b };
    b
}

pub fn cpu_name() -> &'static str {
    match (midr() >> 4) & 0xfff {
        0xD03 => "Cortex-A53",
        0xD08 => "Cortex-A72",
        0xD0B => "Cortex-A76",
        _ => "ARMv8",
    }
}

pub fn sysreg_isar0() -> u64 {
    let v: u64;
    unsafe { asm!("mrs {}, id_aa64isar0_el1", out(reg) v) };
    v
}

// ---------------------------------------------------------------- MMU

#[repr(C, align(4096))]
struct Table([u64; 512]);

static mut L1: Table = Table([0; 512]);
static mut L2: [Table; 4] = [Table([0; 512]), Table([0; 512]), Table([0; 512]), Table([0; 512])];

const ATTR_DEVICE: u64 = 0;
const ATTR_NORMAL: u64 = 1;
const ATTR_NC: u64 = 2;

fn block(addr: u64, attr: u64) -> u64 {
    let af = 1 << 10;
    match attr {
        ATTR_DEVICE => addr | af | (ATTR_DEVICE << 2) | (1 << 54) | (1 << 53) | 0b01,
        a => addr | af | (3 << 8) | (a << 2) | 0b01,
    }
}

/// Mappa identità: 2 MB per i primi 4 GB, 1 GB oltre. RAM normale con cache,
/// periferiche Device, il resto non mappato.
pub fn mmu_init(ram_top: u64) {
    let b = board();
    unsafe {
        let l1 = &mut *core::ptr::addr_of_mut!(L1);
        let l2 = &mut *core::ptr::addr_of_mut!(L2);
        for gb in 0..512u64 {
            let base = gb << 30;
            if gb < 4 {
                let t = &mut l2[gb as usize];
                for i in 0..512u64 {
                    let a = base + (i << 21);
                    t.0[i as usize] = if b.is_device(a) {
                        block(a, ATTR_DEVICE)
                    } else if a < ram_top {
                        block(a, ATTR_NORMAL)
                    } else {
                        0
                    };
                }
                l1.0[gb as usize] = (t.0.as_ptr() as u64) | 0b11;
            } else {
                l1.0[gb as usize] = if b.is_device(base) {
                    block(base, ATTR_DEVICE)
                } else if base < ram_top {
                    block(base, ATTR_NORMAL)
                } else {
                    0
                };
            }
        }
        let mair: u64 = 0x00 | (0xFF << 8) | (0x44 << 16);
        let mmfr0: u64;
        asm!("mrs {}, id_aa64mmfr0_el1", out(reg) mmfr0);
        let ips = (mmfr0 & 0xf).min(5);
        let tcr: u64 = 25            // T0SZ: 39 bit
            | (1 << 8) | (1 << 10)    // cache WB per le tabelle
            | (3 << 12)               // inner shareable
            | (1 << 23)               // EPD1: niente TTBR1
            | (ips << 32);
        asm!(
            "msr mair_el1, {mair}",
            "msr tcr_el1, {tcr}",
            "msr ttbr0_el1, {ttbr}",
            "dsb ish",
            "isb",
            "tlbi vmalle1",
            "dsb ish",
            "isb",
            "mrs {t}, sctlr_el1",
            "orr {t}, {t}, #1",          // M
            "orr {t}, {t}, #(1 << 2)",   // C
            "orr {t}, {t}, #(1 << 12)",  // I
            "msr sctlr_el1, {t}",
            "isb",
            mair = in(reg) mair,
            tcr = in(reg) tcr,
            ttbr = in(reg) core::ptr::addr_of!(L1) as u64,
            t = out(reg) _,
        );
    }
}

/// Rende non-cacheable i blocchi da 2 MB che coprono [addr, addr+len) (framebuffer).
pub fn mmu_uncached(addr: usize, len: usize) {
    unsafe {
        let l2 = &mut *core::ptr::addr_of_mut!(L2);
        let mut a = addr & !((1 << 21) - 1);
        while a < addr + len && a < (1usize << 32) {
            let gb = a >> 30;
            let i = (a >> 21) & 511;
            l2[gb].0[i] = block(a as u64, ATTR_NC);
            a += 1 << 21;
        }
        asm!("dsb ishst", "tlbi vmalle1", "dsb ish", "isb");
    }
}

pub fn cache_clean_inval(addr: usize, len: usize) {
    let mut a = addr & !63;
    while a < addr + len {
        unsafe { asm!("dc civac, {}", in(reg) a) };
        a += 64;
    }
    unsafe { asm!("dsb sy") };
}

// ---------------------------------------------------------------- UART

fn uart_reg(off: usize) -> *mut u32 {
    (board().uart_base() + off) as *mut u32
}

pub fn uart_init() {
    let b = board();
    unsafe {
        if let Some(g) = b.gpio_base() {
            // GPIO14/15 in ALT0 (TXD0/RXD0)
            let fsel1 = (g + 0x04) as *mut u32;
            let mut v = read_volatile(fsel1);
            v &= !((7 << 12) | (7 << 15));
            v |= (4 << 12) | (4 << 15);
            write_volatile(fsel1, v);
        }
        if b == Board::Pi5 && read_volatile(uart_reg(0x30)) & 1 != 0 {
            // il firmware (enable_uart=1) ha già impostato la UART di debug: teniamo i suoi baud,
            // perché il clock di riferimento del Pi 5 non è documentato con certezza
            write_volatile(uart_reg(0x38), 0);
            return;
        }
        write_volatile(uart_reg(0x30), 0); // CR: spento
        write_volatile(uart_reg(0x44), 0x7ff); // ICR
        // 115200 baud: clock 48 MHz su Pi 3/4 (init_uart_clock); sul Pi 5 44,2368 MHz misurati (da confermare)
        let (ib, fb) = if b == Board::Pi5 { (24, 0) } else { (26, 3) };
        write_volatile(uart_reg(0x24), ib);
        write_volatile(uart_reg(0x28), fb);
        write_volatile(uart_reg(0x2C), (3 << 5) | (1 << 4)); // 8N1, FIFO
        write_volatile(uart_reg(0x38), 0); // niente interruzioni
        write_volatile(uart_reg(0x30), (1 << 0) | (1 << 8) | (1 << 9));
    }
}

pub fn uart_putb(c: u8) {
    unsafe {
        let mut n = 0u32;
        while read_volatile(uart_reg(0x18)) & (1 << 5) != 0 {
            n += 1;
            if n > 1_000_000 {
                return; // nessuno legge: non bloccare il sistema
            }
        }
        write_volatile(uart_reg(0x00), c as u32);
    }
}

pub fn uart_puts(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            uart_putb(b'\r');
        }
        uart_putb(b);
    }
}

pub fn uart_getb() -> Option<u8> {
    unsafe {
        if read_volatile(uart_reg(0x18)) & (1 << 4) != 0 {
            None
        } else {
            Some((read_volatile(uart_reg(0x00)) & 0xff) as u8)
        }
    }
}

pub struct Uart;
impl core::fmt::Write for Uart {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        uart_puts(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! log {
    ($($a:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::plat::Uart, $($a)*);
    }};
}

// ---------------------------------------------------------------- mailbox

#[repr(C, align(64))]
struct MboxBuf([u32; 64]);
static mut MBOX: MboxBuf = MboxBuf([0; 64]);

/// Esegue una richiesta di proprietà (canale 8). `msg` contiene i tag già composti,
/// senza intestazione né tag finale; al ritorno contiene le risposte.
pub fn mbox_call(msg: &mut [u32]) -> bool {
    let b = board();
    let base = b.mbox_base();
    unsafe {
        let buf = &mut *core::ptr::addr_of_mut!(MBOX);
        let n = msg.len();
        if n + 3 > buf.0.len() {
            return false;
        }
        buf.0[0] = ((n + 3) * 4) as u32;
        buf.0[1] = 0;
        buf.0[2..2 + n].copy_from_slice(msg);
        buf.0[2 + n] = 0;
        let addr = buf.0.as_ptr() as usize;
        cache_clean_inval(addr, 256);
        let status = (base + 0x18) as *const u32; // mailbox 0 (ARM legge)
        let wstatus = (base + 0x38) as *const u32; // mailbox 1 (ARM scrive)
        let write = (base + 0x20) as *mut u32;
        let read = base as *const u32;
        let mut guard = 0u32;
        while read_volatile(wstatus) & 0x8000_0000 != 0 {
            guard += 1;
            if guard > 10_000_000 {
                return false;
            }
        }
        let req = (b.bus(addr) & !0xf) | 8;
        write_volatile(write, req);
        loop {
            guard = 0;
            while read_volatile(status) & 0x4000_0000 != 0 {
                guard += 1;
                if guard > 50_000_000 {
                    return false;
                }
            }
            if read_volatile(read) == req {
                break;
            }
        }
        cache_clean_inval(addr, 256);
        msg.copy_from_slice(&buf.0[2..2 + n]);
        buf.0[1] == 0x8000_0000
    }
}

/// Tag con un valore in uscita e fino a 2 in ingresso.
fn mbox_get(tag: u32, a: u32, b2: u32) -> Option<(u32, u32)> {
    let mut m = [tag, 8, 0, a, b2];
    if mbox_call(&mut m) && m[2] & 0x8000_0000 != 0 {
        Some((m[3], m[4]))
    } else {
        None
    }
}

pub fn board_revision() -> u32 {
    mbox_get(0x0001_0002, 0, 0).map(|v| v.0).unwrap_or(0)
}

pub fn board_serial() -> u64 {
    mbox_get(0x0001_0004, 0, 0).map(|v| (v.1 as u64) << 32 | v.0 as u64).unwrap_or(0)
}

/// (base, dimensione) della RAM vista dall'ARM nel primo GB
pub fn arm_memory() -> (u32, u32) {
    mbox_get(0x0001_0005, 0, 0).unwrap_or((0, 0x3c00_0000))
}

/// temperatura del SoC in millesimi di °C
pub fn temperature() -> Option<u32> {
    mbox_get(0x0003_0006, 0, 0).map(|v| v.1)
}

pub fn temperature_max() -> Option<u32> {
    mbox_get(0x0003_000A, 0, 0).map(|v| v.1)
}

/// frequenza del clock ARM in Hz (misurata se possibile)
pub fn arm_clock() -> Option<u32> {
    mbox_get(0x0003_0047, 3, 0).or_else(|| mbox_get(0x0003_0002, 3, 0)).map(|v| v.1).filter(|&v| v != 0)
}

/// memoria totale dal codice di revisione (formato nuovo)
pub fn total_memory(rev: u32) -> u64 {
    if rev & (1 << 23) == 0 {
        return 1 << 30;
    }
    256u64 << 20 << ((rev >> 20) & 7)
}

pub fn model_name(rev: u32) -> &'static str {
    if rev & (1 << 23) == 0 {
        return board().name();
    }
    match (rev >> 4) & 0xff {
        0x04 => "Raspberry Pi 2 B",
        0x08 => "Raspberry Pi 3 B",
        0x0D => "Raspberry Pi 3 B+",
        0x0E => "Raspberry Pi 3 A+",
        0x11 => "Raspberry Pi 4 B",
        0x12 => "Raspberry Pi Zero 2 W",
        0x13 => "Raspberry Pi 400",
        0x14 => "Compute Module 4",
        0x17 => "Raspberry Pi 5",
        0x18 => "Compute Module 5",
        0x19 => "Raspberry Pi 500",
        _ => board().name(),
    }
}

pub struct Fb {
    pub ptr: *mut u32,
    pub w: usize,
    pub h: usize,
    pub pitch: usize,
    /// true se il byte basso è il rosso (ordine RGB del firmware)
    pub rgb: bool,
}

pub fn framebuffer(w: u32, h: u32) -> Option<Fb> {
    framebuffer_set(w, h).or_else(framebuffer_current)
}

/// Chiede una risoluzione precisa (Pi 3/4; il firmware del Pi 5 può rifiutare).
fn framebuffer_set(w: u32, h: u32) -> Option<Fb> {
    let mut m = [
        0x0004_8003, 8, 0, w, h, // dimensione fisica
        0x0004_8004, 8, 0, w, h, // dimensione virtuale
        0x0004_8009, 8, 0, 0, 0, // offset virtuale
        0x0004_8005, 4, 0, 32, // profondità
        0x0004_8006, 4, 0, 0, // ordine dei pixel: 0 = BGR, come da predefinito
        0x0004_0001, 8, 0, 4096, 0, // alloca
        0x0004_0008, 4, 0, 0, // pitch
    ];
    if !mbox_call(&mut m) {
        return None;
    }
    make_fb(m[3], m[4], m[18], m[22], m[26], m[31])
}

/// Usa la modalità già impostata dal firmware sull'HDMI (caso tipico del Pi 5).
fn framebuffer_current() -> Option<Fb> {
    let mut m = [
        0x0004_8005, 4, 0, 32, // profondità
        0x0004_0001, 8, 0, 4096, 0, // alloca
        0x0004_0003, 8, 0, 0, 0, // dimensione fisica attuale
        0x0004_0008, 4, 0, 0, // pitch
        0x0004_0006, 4, 0, 0, // ordine dei pixel
    ];
    if !mbox_call(&mut m) {
        return None;
    }
    make_fb(m[12], m[13], m[3], m[21], m[7], m[17])
}

fn make_fb(fw: u32, fh: u32, depth: u32, order: u32, addr: u32, pitch: u32) -> Option<Fb> {
    let addr = (addr & 0x3FFF_FFFF) as usize;
    let (fw, fh) = (fw as usize, fh as usize);
    if addr == 0 || depth != 32 || fw == 0 || fh == 0 {
        return None;
    }
    let pitch = pitch as usize;
    Some(Fb { ptr: addr as *mut u32, w: fw, h: fh, pitch: if pitch == 0 { fw * 4 } else { pitch }, rgb: order == 1 })
}

// ---------------------------------------------------------------- timer

pub fn ticks() -> u64 {
    let v: u64;
    unsafe { asm!("isb", "mrs {}, cntpct_el0", out(reg) v) };
    v
}

pub fn freq() -> u64 {
    let v: u64;
    unsafe { asm!("mrs {}, cntfrq_el0", out(reg) v) };
    if v == 0 {
        19_200_000
    } else {
        v
    }
}

pub fn millis() -> u64 {
    ticks() * 1000 / freq()
}

pub fn delay_ms(ms: u64) {
    let end = ticks() + ms * freq() / 1000;
    while ticks() < end {
        core::hint::spin_loop();
    }
}
