//! Piattaforma PC x86_64: multiboot2, seriale COM1, tastiera PS/2, timer (TSC tarato col PIT),
//! orologio CMOS, CPUID, SMBIOS, eccezioni della CPU.

use alloc::string::String;
use alloc::vec::Vec;
use core::arch::asm;

// ---------------------------------------------------------------- porte

#[inline]
pub fn outb(port: u16, v: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") v, options(nomem, nostack, preserves_flags)) };
}

#[inline]
pub fn outw(port: u16, v: u16) {
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") v, options(nomem, nostack, preserves_flags)) };
}

#[inline]
pub fn inb(port: u16) -> u8 {
    let v: u8;
    unsafe { asm!("in al, dx", out("al") v, in("dx") port, options(nomem, nostack, preserves_flags)) };
    v
}

fn io_wait() {
    outb(0x80, 0);
}

// ---------------------------------------------------------------- seriale COM1

const COM1: u16 = 0x3F8;
static mut SERIAL_OK: bool = false;

pub fn uart_init() {
    outb(COM1 + 1, 0x00); // niente interruzioni
    outb(COM1 + 3, 0x80); // DLAB
    outb(COM1, 0x01); // 115200 baud
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x03); // 8N1
    outb(COM1 + 2, 0xC7); // FIFO
    outb(COM1 + 4, 0x03);
    // se la porta non c'è, la linea di stato legge 0xFF
    unsafe { SERIAL_OK = inb(COM1 + 5) != 0xFF };
}

pub fn uart_putb(c: u8) {
    if unsafe { !SERIAL_OK } {
        return;
    }
    let mut n = 0u32;
    while inb(COM1 + 5) & 0x20 == 0 {
        n += 1;
        if n > 100_000 {
            return;
        }
    }
    outb(COM1, c);
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
    if unsafe { SERIAL_OK } && inb(COM1 + 5) & 1 != 0 {
        Some(inb(COM1))
    } else {
        None
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

// ---------------------------------------------------------------- tastiera PS/2

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    It,
    Us,
}

struct Kbd {
    shift: bool,
    ctrl: bool,
    altgr: bool,
    caps: bool,
    e0: bool,
    layout: Layout,
    buf: [u8; 64],
    head: usize,
    tail: usize,
}

static mut KBD: Kbd = Kbd { shift: false, ctrl: false, altgr: false, caps: false, e0: false, layout: Layout::It, buf: [0; 64], head: 0, tail: 0 };

fn kbd() -> &'static mut Kbd {
    unsafe { &mut *core::ptr::addr_of_mut!(KBD) }
}

pub fn set_layout(l: Layout) {
    kbd().layout = l;
}

pub fn layout() -> Layout {
    kbd().layout
}

pub fn kbd_init() {
    // svuota il buffer del controller
    for _ in 0..32 {
        if inb(0x64) & 1 == 0 {
            break;
        }
        let _ = inb(0x60);
    }
}

fn push(s: &str) {
    let k = kbd();
    for b in s.bytes() {
        let n = (k.head + 1) % k.buf.len();
        if n == k.tail {
            return;
        }
        k.buf[k.head] = b;
        k.head = n;
    }
}

/// (normale, maiuscolo, AltGr, Maiusc+AltGr) per la tastiera italiana, set di scancode 1
fn it_key(sc: u8) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    Some(match sc {
        0x02 => ("1", "!", "", ""),
        0x03 => ("2", "\"", "", ""),
        0x04 => ("3", "£", "", ""),
        0x05 => ("4", "$", "", ""),
        0x06 => ("5", "%", "€", ""),
        0x07 => ("6", "&", "", ""),
        0x08 => ("7", "/", "{", ""),
        0x09 => ("8", "(", "[", ""),
        0x0A => ("9", ")", "]", ""),
        0x0B => ("0", "=", "}", ""),
        0x0C => ("'", "?", "`", ""),
        0x0D => ("ì", "^", "~", ""),
        0x1A => ("è", "é", "[", "{"),
        0x1B => ("+", "*", "]", "}"),
        0x27 => ("ò", "ç", "@", ""),
        0x28 => ("à", "°", "#", ""),
        0x29 => ("\\", "|", "", ""),
        0x2B => ("ù", "§", "", ""),
        0x33 => (",", ";", "", ""),
        0x34 => (".", ":", "", ""),
        0x35 => ("-", "_", "", ""),
        0x56 => ("<", ">", "", ""),
        0x12 => ("e", "E", "€", ""),
        _ => return None,
    })
}

fn us_key(sc: u8) -> Option<(&'static str, &'static str)> {
    Some(match sc {
        0x02 => ("1", "!"),
        0x03 => ("2", "@"),
        0x04 => ("3", "#"),
        0x05 => ("4", "$"),
        0x06 => ("5", "%"),
        0x07 => ("6", "^"),
        0x08 => ("7", "&"),
        0x09 => ("8", "*"),
        0x0A => ("9", "("),
        0x0B => ("0", ")"),
        0x0C => ("-", "_"),
        0x0D => ("=", "+"),
        0x1A => ("[", "{"),
        0x1B => ("]", "}"),
        0x27 => (";", ":"),
        0x28 => ("'", "\""),
        0x29 => ("`", "~"),
        0x2B => ("\\", "|"),
        0x33 => (",", "<"),
        0x34 => (".", ">"),
        0x35 => ("/", "?"),
        0x56 => ("\\", "|"),
        _ => return None,
    })
}

fn letter(sc: u8) -> Option<u8> {
    const ROWS: [(u8, &[u8]); 3] = [(0x10, b"qwertyuiop"), (0x1E, b"asdfghjkl"), (0x2C, b"zxcvbnm")];
    for (start, row) in ROWS {
        if sc >= start && ((sc - start) as usize) < row.len() {
            return Some(row[(sc - start) as usize]);
        }
    }
    None
}

fn scancode(sc: u8) {
    let k = kbd();
    if sc == 0xE0 {
        k.e0 = true;
        return;
    }
    let e0 = core::mem::replace(&mut k.e0, false);
    let up = sc & 0x80 != 0;
    let code = sc & 0x7F;
    match (e0, code) {
        (false, 0x2A) | (false, 0x36) => {
            k.shift = !up;
            return;
        }
        (_, 0x1D) => {
            k.ctrl = !up;
            return;
        }
        (true, 0x38) => {
            k.altgr = !up;
            return;
        }
        (false, 0x38) => return,
        (false, 0x3A) => {
            if !up {
                k.caps = !k.caps;
            }
            return;
        }
        _ => {}
    }
    if up {
        return;
    }
    if e0 {
        // invio del tastierino; le frecce sono ignorate
        if code == 0x1C {
            push("\r");
        }
        return;
    }
    match code {
        0x1C => return push("\r"),
        0x0E => return push("\x08"),
        0x39 => return push(" "),
        0x0F => return push(" "),
        0x01 => return, // Esc: niente (la riga principale tratta 0x1b come sequenza ANSI)
        _ => {}
    }
    if k.ctrl && code == 0x2E {
        return push("\x03");
    }
    if k.layout == Layout::It {
        if let Some((n, s, ag, sag)) = it_key(code) {
            let out = if k.altgr && k.shift && !sag.is_empty() {
                sag
            } else if k.altgr && !ag.is_empty() {
                ag
            } else if k.altgr {
                ""
            } else if k.shift {
                s
            } else {
                n
            };
            if !out.is_empty() {
                // "e" senza modificatori passa sotto, come lettera normale (con Bloc Maiusc)
                if !(code == 0x12 && !k.altgr) {
                    return push(out);
                }
            } else {
                return;
            }
        }
    } else if let Some((n, s)) = us_key(code) {
        return push(if k.shift { s } else { n });
    }
    if let Some(c) = letter(code) {
        let upper = k.shift ^ k.caps;
        let c = if upper { c.to_ascii_uppercase() } else { c };
        let mut b = [0u8; 1];
        b[0] = c;
        push(core::str::from_utf8(&b).unwrap_or(""));
    }
}

/// Legge un byte UTF-8 dalla tastiera PS/2 (prima scarica il controller).
pub fn kbd_getb() -> Option<u8> {
    for _ in 0..16 {
        let st = inb(0x64);
        if st & 1 == 0 {
            break;
        }
        let d = inb(0x60);
        if st & 0x20 != 0 {
            continue; // dato del mouse
        }
        scancode(d);
    }
    let k = kbd();
    if k.tail == k.head {
        return None;
    }
    let b = k.buf[k.tail];
    k.tail = (k.tail + 1) % k.buf.len();
    Some(b)
}

// ---------------------------------------------------------------- timer

#[inline]
pub fn ticks() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

static mut TSC_KHZ: u64 = 1_000_000;

/// Taratura del TSC con il canale 2 del PIT (50 ms).
pub fn timer_init() {
    const PIT_HZ: u64 = 1_193_182;
    let count: u16 = (PIT_HZ * 50 / 1000) as u16;
    let p61 = inb(0x61);
    outb(0x61, (p61 & 0xFD) | 0x01); // gate on, altoparlante off
    outb(0x43, 0xB0); // canale 2, modo 0, lsb/msb
    outb(0x42, (count & 0xff) as u8);
    outb(0x42, (count >> 8) as u8);
    let t0 = ticks();
    let mut guard = 0u64;
    while inb(0x61) & 0x20 == 0 {
        guard += 1;
        if guard > 50_000_000 {
            break;
        }
    }
    let dt = ticks() - t0;
    outb(0x61, p61);
    let khz = dt / 50;
    unsafe { TSC_KHZ = if khz < 1000 { 1_000_000 } else { khz } };
}

pub fn tsc_mhz() -> u64 {
    unsafe { TSC_KHZ / 1000 }
}

pub fn millis() -> u64 {
    ticks() / unsafe { TSC_KHZ }
}

pub fn delay_ms(ms: u64) {
    let end = millis() + ms;
    while millis() < end {
        core::hint::spin_loop();
    }
}

pub fn temperature() -> Option<u32> {
    None // sensori termici (MSR/ACPI) non ancora letti
}

// ---------------------------------------------------------------- orologio CMOS

fn cmos(reg: u8) -> u8 {
    outb(0x70, reg);
    io_wait();
    inb(0x71)
}

/// (ore, minuti, secondi) dall'RTC
pub fn rtc_time() -> Option<(u8, u8, u8)> {
    for _ in 0..1000 {
        if cmos(0x0A) & 0x80 == 0 {
            break;
        }
    }
    let (mut s, mut m, mut h) = (cmos(0x00), cmos(0x02), cmos(0x04));
    let b = cmos(0x0B);
    if b & 0x04 == 0 {
        let bcd = |v: u8| (v & 0x0F) + (v >> 4) * 10;
        s = bcd(s);
        m = bcd(m);
        h = bcd(h & 0x7F) | (h & 0x80);
    }
    if b & 0x02 == 0 && h & 0x80 != 0 {
        h = ((h & 0x7F) + 12) % 24;
    }
    if h > 23 || m > 59 || s > 59 {
        return None;
    }
    Some((h, m, s))
}

// ---------------------------------------------------------------- CPUID

pub fn cpuid(leaf: u32, sub: u32) -> (u32, u32, u32, u32) {
    #[allow(unused_unsafe)]
    let r = unsafe { core::arch::x86_64::__cpuid_count(leaf, sub) };
    (r.eax, r.ebx, r.ecx, r.edx)
}

pub fn cpu_vendor() -> String {
    let (_, b, c, d) = cpuid(0, 0);
    let mut v = Vec::new();
    for r in [b, d, c] {
        v.extend_from_slice(&r.to_le_bytes());
    }
    String::from_utf8_lossy(&v).into_owned()
}

pub fn cpu_name() -> String {
    let (max, _, _, _) = cpuid(0x8000_0000, 0);
    if max >= 0x8000_0004 {
        let mut v = Vec::new();
        for l in 0x8000_0002..=0x8000_0004u32 {
            let (a, b, c, d) = cpuid(l, 0);
            for r in [a, b, c, d] {
                v.extend_from_slice(&r.to_le_bytes());
            }
        }
        let s = String::from_utf8_lossy(&v).into_owned();
        let s = s.trim_matches(|c: char| c == '\0' || c == ' ');
        if !s.is_empty() {
            // spazi multipli -> uno
            let mut out = String::new();
            let mut sp = false;
            for ch in s.chars() {
                if ch == ' ' {
                    if !sp {
                        out.push(' ');
                    }
                    sp = true;
                } else {
                    out.push(ch);
                    sp = false;
                }
            }
            return out;
        }
    }
    alloc::format!("CPU {}", cpu_vendor())
}

/// firma (famiglia/modello/stepping) del processore
pub fn cpu_signature() -> u32 {
    cpuid(1, 0).0
}

/// processori logici dichiarati da CPUID
pub fn cpu_threads() -> u32 {
    let (max, _, _, _) = cpuid(0, 0);
    if max >= 0x0B {
        let (_, b, _, _) = cpuid(0x0B, 1);
        if b & 0xffff != 0 {
            return b & 0xffff;
        }
    }
    let (_, b, _, d) = cpuid(1, 0);
    if d & (1 << 28) != 0 {
        ((b >> 16) & 0xff).max(1)
    } else {
        1
    }
}

pub struct Features {
    pub sse42: bool,
    pub avx: bool,
    pub avx2: bool,
    pub fma: bool,
    pub avx512: bool,
    pub hypervisor: bool,
}

pub fn features() -> Features {
    let (max, _, _, _) = cpuid(0, 0);
    let (_, _, c1, _) = cpuid(1, 0);
    let b7 = if max >= 7 { cpuid(7, 0).1 } else { 0 };
    Features {
        sse42: c1 & (1 << 20) != 0,
        avx: c1 & (1 << 28) != 0,
        fma: c1 & (1 << 12) != 0,
        avx2: b7 & (1 << 5) != 0,
        avx512: b7 & (1 << 16) != 0,
        hypervisor: c1 & (1 << 31) != 0,
    }
}

// ---------------------------------------------------------------- SMBIOS

pub struct Smbios {
    pub vendor: String,
    pub product: String,
    pub uuid: [u8; 16],
}

fn smbios_str(table: *const u8, len: usize, idx: u8) -> String {
    if idx == 0 {
        return String::new();
    }
    let mut p = unsafe { table.add(len) };
    let mut n = 1u8;
    unsafe {
        while n < idx {
            if *p == 0 {
                return String::new();
            }
            while *p != 0 {
                p = p.add(1);
            }
            p = p.add(1);
            n += 1;
        }
        let mut v = Vec::new();
        while *p != 0 && v.len() < 64 {
            v.push(*p);
            p = p.add(1);
        }
        String::from(String::from_utf8_lossy(&v).trim())
    }
}

/// Cerca la tabella SMBIOS (punto d'ingresso a 32 o 64 bit) nell'area del BIOS e legge il tipo 1 (sistema).
pub fn smbios() -> Option<Smbios> {
    let mut a = 0xF0000usize;
    while a < 0x100000 {
        let p = a as *const u8;
        let sig = unsafe { core::slice::from_raw_parts(p, 5) };
        let found = if &sig[..4] == b"_SM_" {
            let table = unsafe { core::ptr::read_unaligned((a + 0x18) as *const u32) } as usize;
            let tlen = unsafe { core::ptr::read_unaligned((a + 0x16) as *const u16) } as usize;
            Some((table, tlen))
        } else if sig == b"_SM3_" {
            let table = unsafe { core::ptr::read_unaligned((a + 0x10) as *const u64) } as usize;
            let tlen = unsafe { core::ptr::read_unaligned((a + 0x0C) as *const u32) } as usize;
            Some((table, tlen))
        } else {
            None
        };
        if let Some((table, tlen)) = found {
            if table == 0 || table >= 0xFFFF_0000 || tlen == 0 {
                return None;
            }
            let mut q = table;
            let end = table + tlen;
            while q + 4 <= end {
                let t = unsafe { *(q as *const u8) };
                let l = unsafe { *((q + 1) as *const u8) } as usize;
                if l < 4 {
                    break;
                }
                if t == 1 && l >= 0x19 {
                    let mut uuid = [0u8; 16];
                    uuid.copy_from_slice(unsafe { core::slice::from_raw_parts((q + 8) as *const u8, 16) });
                    let vendor = smbios_str(q as *const u8, l, unsafe { *((q + 4) as *const u8) });
                    let product = smbios_str(q as *const u8, l, unsafe { *((q + 5) as *const u8) });
                    return Some(Smbios { vendor, product, uuid });
                }
                if t == 127 {
                    break;
                }
                // salta la parte formattata e le stringhe (finisce con due zeri)
                let mut s = q + l;
                while s + 1 < end && unsafe { *(s as *const u8) != 0 || *((s + 1) as *const u8) != 0 } {
                    s += 1;
                }
                q = s + 2;
            }
            return None;
        }
        a += 16;
    }
    None
}

// ---------------------------------------------------------------- multiboot2

pub struct Fb {
    pub ptr: *mut u32,
    pub w: usize,
    pub h: usize,
    pub pitch: usize,
    /// true se il byte basso è il rosso
    pub rgb: bool,
}

pub struct BootInfo {
    pub fb: Option<Fb>,
    /// regioni di RAM utilizzabile (inizio, fine)
    pub ram: Vec<(u64, u64)>,
    pub ram_total: u64,
    pub modules: Vec<(usize, usize, String)>,
    pub loader: String,
    pub cmdline: String,
}

fn cstr(p: *const u8, max: usize) -> String {
    let mut v = Vec::new();
    unsafe {
        let mut i = 0;
        while i < max && *p.add(i) != 0 {
            v.push(*p.add(i));
            i += 1;
        }
    }
    String::from_utf8_lossy(&v).into_owned()
}

/// Prima passata senza heap: solo i moduli e la fine dell'area più alta da proteggere.
pub fn mb2_reserved_end(info: usize) -> usize {
    let total = unsafe { *(info as *const u32) } as usize;
    let mut end = info + total;
    let mut p = info + 8;
    while p < info + total {
        let typ = unsafe { *(p as *const u32) };
        let size = unsafe { *((p + 4) as *const u32) } as usize;
        if typ == 0 {
            break;
        }
        if typ == 3 {
            let me = unsafe { *((p + 12) as *const u32) } as usize;
            end = end.max(me);
        }
        p += (size + 7) & !7;
    }
    end
}

/// RAM più alta utilizzabile sotto i 4 GiB, che contiene `from` (per l'heap).
pub fn mb2_ram_limit(info: usize, from: usize) -> usize {
    let total = unsafe { *(info as *const u32) } as usize;
    let mut p = info + 8;
    let mut best = 0usize;
    while p < info + total {
        let typ = unsafe { *(p as *const u32) };
        let size = unsafe { *((p + 4) as *const u32) } as usize;
        if typ == 0 {
            break;
        }
        if typ == 6 {
            let esz = unsafe { *((p + 8) as *const u32) } as usize;
            let mut e = p + 16;
            while e + esz <= p + size {
                let base = unsafe { core::ptr::read_unaligned(e as *const u64) };
                let len = unsafe { core::ptr::read_unaligned((e + 8) as *const u64) };
                let t = unsafe { core::ptr::read_unaligned((e + 16) as *const u32) };
                let top = (base + len).min(0xFFFF_F000) as usize;
                if t == 1 && (base as usize) <= from && top > from {
                    best = best.max(top);
                }
                e += esz;
            }
        }
        p += (size + 7) & !7;
    }
    best
}

pub fn mb2_parse(info: usize) -> BootInfo {
    let mut bi = BootInfo { fb: None, ram: Vec::new(), ram_total: 0, modules: Vec::new(), loader: String::new(), cmdline: String::new() };
    let total = unsafe { *(info as *const u32) } as usize;
    let mut p = info + 8;
    while p < info + total {
        let typ = unsafe { *(p as *const u32) };
        let size = unsafe { *((p + 4) as *const u32) } as usize;
        match typ {
            0 => break,
            1 => bi.cmdline = cstr((p + 8) as *const u8, size - 8),
            2 => bi.loader = cstr((p + 8) as *const u8, size - 8),
            3 => {
                let s = unsafe { *((p + 8) as *const u32) } as usize;
                let e = unsafe { *((p + 12) as *const u32) } as usize;
                bi.modules.push((s, e, cstr((p + 16) as *const u8, size - 16)));
            }
            6 => {
                let esz = unsafe { *((p + 8) as *const u32) } as usize;
                let mut e = p + 16;
                while e + esz <= p + size {
                    let base = unsafe { core::ptr::read_unaligned(e as *const u64) };
                    let len = unsafe { core::ptr::read_unaligned((e + 8) as *const u64) };
                    let t = unsafe { core::ptr::read_unaligned((e + 16) as *const u32) };
                    if t == 1 {
                        bi.ram.push((base, base + len));
                        bi.ram_total += len;
                    }
                    e += esz;
                }
            }
            8 => {
                let addr = unsafe { core::ptr::read_unaligned((p + 8) as *const u64) } as usize;
                let pitch = unsafe { *((p + 16) as *const u32) } as usize;
                let w = unsafe { *((p + 20) as *const u32) } as usize;
                let h = unsafe { *((p + 24) as *const u32) } as usize;
                let bpp = unsafe { *((p + 28) as *const u8) };
                let ftype = unsafe { *((p + 29) as *const u8) };
                if ftype == 1 && bpp == 32 && addr != 0 && addr < 0x1_0000_0000 {
                    let red_pos = unsafe { *((p + 32) as *const u8) };
                    bi.fb = Some(Fb { ptr: addr as *mut u32, w, h, pitch, rgb: red_pos == 0 });
                }
            }
            _ => {}
        }
        p += (size + 7) & !7;
    }
    bi
}

// ---------------------------------------------------------------- eccezioni

#[repr(C, packed)]
struct IdtPtr {
    limit: u16,
    base: u64,
}

#[repr(C, align(16))]
struct Idt([u64; 512]);
static mut IDT: Idt = Idt([0; 512]);

extern "C" {
    static isr_table: [u64; 21];
}

pub fn idt_init() {
    unsafe {
        let idt = &mut *core::ptr::addr_of_mut!(IDT);
        for (i, &h) in (*core::ptr::addr_of!(isr_table)).iter().enumerate() {
            let lo = (h & 0xffff) | (0x08 << 16) | (0x8E00u64 << 32) | (((h >> 16) & 0xffff) << 48);
            let hi = h >> 32;
            idt.0[i * 2] = lo;
            idt.0[i * 2 + 1] = hi;
        }
        let ptr = IdtPtr { limit: (256 * 16 - 1) as u16, base: idt.0.as_ptr() as u64 };
        asm!("lidt [{}]", in(reg) &ptr, options(readonly, nostack));
    }
}

pub fn halt() -> ! {
    loop {
        unsafe { asm!("cli; hlt", options(nomem, nostack)) };
    }
}

/// Spegnimento nelle macchine virtuali: porte ACPI di Bochs/QEMU vecchio (0xB004),
/// QEMU (0x604), VirtualBox (0x4004) e la porta «Shutdown» di Bochs (0x8900).
/// Sul PC reale serve l'ACPI completo (FADT + _S5 dalla DSDT): torna `false` e il chiamante ferma la CPU.
pub fn poweroff() -> bool {
    outw(0xB004, 0x2000);
    outw(0x604, 0x2000);
    outw(0x4004, 0x3400);
    for &c in b"Shutdown" {
        outb(0x8900, c);
    }
    false
}

pub fn reboot() -> ! {
    // impulso di reset dal controller della tastiera
    for _ in 0..100000 {
        if inb(0x64) & 2 == 0 {
            break;
        }
    }
    outb(0x64, 0xFE);
    halt()
}
