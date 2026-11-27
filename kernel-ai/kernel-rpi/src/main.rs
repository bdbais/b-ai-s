//! kernel-ai di B-AI-S per Raspberry Pi 5 — prototipo v0.3-rpi.
//!
//! Stessa idea della v0.2 x86: il kernel disegna un desktop che si modifica da frase,
//! con diario, abitudini, brain in RAM e il gestore modelli `bais-model` agganciato.
//! Si parla con il sistema dalla UART di debug; il desktop esce su HDMI.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::global_asm;

mod fdt;
mod gfx;
mod heap;
mod hw;
mod interp;
mod plat;
mod ui;

global_asm!(include_str!("boot.s"));

extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
}

struct Screen {
    fb: plat::Fb,
    canvas: gfx::Canvas,
    fonts: ui::Fonts,
}

static mut SCREEN: Option<Screen> = None;

fn screen() -> Option<&'static mut Screen> {
    unsafe { (*core::ptr::addr_of_mut!(SCREEN)).as_mut() }
}

const VERSION: &str = "0.3.0-rpi";

/// Estensione di un GGUF senza conoscerne la dimensione (modulo a indirizzo fisso).
fn gguf_extent(addr: usize, max: usize) -> Option<usize> {
    let buf = unsafe { core::slice::from_raw_parts(addr as *const u8, max) };
    if &buf[..4] != b"GGUF" {
        return None;
    }
    let g = bais_model::gguf::Gguf::parse(buf).ok()?;
    let start = max - g.data.len();
    let mut end = start;
    for t in &g.tensors {
        let (be, bb) = t.dtype.block()?;
        let n: usize = t.ne.iter().take(t.n_dims.max(1)).product();
        end = end.max(start + t.offset + n / be * bb);
    }
    Some(end)
}

fn refresh_sys(s: &mut interp::Sys) {
    s.desk.sys = hw::sys_lines(s);
    let up = plat::millis() / 1000;
    let temp = plat::temperature().map(|t| format!("{} °C   ", t / 1000)).unwrap_or_default();
    s.desk.tray = format!("{}{:02}:{:02}:{:02}", temp, up / 3600, (up / 60) % 60, up % 60);
}

fn redraw(s: &mut interp::Sys) {
    if let Some(sc) = screen() {
        ui::render(&mut sc.canvas, &sc.fonts, &s.desk);
        sc.canvas.present(&sc.fb, 0, 0, 0, 0);
    }
}

fn redraw_prompt(s: &interp::Sys) {
    if let Some(sc) = screen() {
        let (x, y, w, h) = ui::prompt_rect(&sc.canvas, &s.desk);
        ui::draw_prompt(&mut sc.canvas, &sc.fonts, &s.desk, x, y, w, h);
        sc.canvas.present(&sc.fb, x - 20, y - 12, w + 40, h + 24);
    }
}

#[no_mangle]
pub extern "C" fn kmain(dtb: usize) -> ! {
    let board = plat::detect();
    plat::uart_init();
    log!("\n\nB-AI-S kernel-ai {} — {} ({})\n", VERSION, board.name(), plat::cpu_name());

    let rev = plat::board_revision();
    let total = plat::total_memory(rev);
    let arm = plat::arm_memory();
    log!("revisione {:08x}, memoria {} MB, ARM {} MB\n", rev, total >> 20, arm.1 >> 20);
    plat::mmu_init(total.max(arm.0 as u64 + arm.1 as u64));
    log!("MMU attiva\n");

    // device tree: modello della scheda e initramfs (il file del modello)
    let dt = fdt::parse(dtb);
    let mut model_region: Option<(usize, usize)> = None;
    if let Some(d) = &dt {
        model_region = d.initrd;
        log!("device tree: «{}», initrd {:x?}\n", core::str::from_utf8(&d.model[..d.model_len]).unwrap_or(""), d.initrd);
    } else {
        log!("nessun device tree\n");
    }
    let arm_end = (arm.0 + arm.1) as usize;
    if model_region.is_none() {
        // ripiego: modello a indirizzo fisso (initramfs bais-model.gguf 0x08000000)
        let fixed = 0x0800_0000usize;
        let kend0 = core::ptr::addr_of!(__kernel_end) as usize;
        if fixed < arm_end && unsafe { core::slice::from_raw_parts(fixed as *const u8, 4) } == b"GGUF" {
            // heap provvisorio sotto il modello, solo per leggerne l'intestazione
            heap::init(kend0, fixed);
            if let Some(n) = gguf_extent(fixed, arm_end - fixed - (1 << 20)) {
                model_region = Some((fixed, fixed + n));
                log!("modello trovato a {:#x} ({} byte)\n", fixed, n);
            }
        }
    }

    // heap: dopo il kernel e dopo il modello, fino alla fine della RAM dell'ARM
    let kend = core::ptr::addr_of!(__kernel_end) as usize;
    let mut hstart = kend;
    if let Some((_, e)) = model_region {
        hstart = hstart.max(e);
    }
    let hend = arm_end.saturating_sub(1 << 20).min(0x3B00_0000);
    heap::init(hstart, hend);
    log!("heap {:#x}..{:#x} ({} MB)\n", hstart, hend, (hend - hstart) >> 20);
    // da qui in poi si può allocare
    let dt_model = dt.as_ref().map(|d| String::from(core::str::from_utf8(&d.model[..d.model_len]).unwrap_or(""))).unwrap_or_default();

    // schermo
    let fb = plat::framebuffer(1280, 720);
    match &fb {
        Some(f) => {
            plat::mmu_uncached(f.ptr as usize, f.pitch * f.h);
            log!("framebuffer {}x{} a {:p}, ordine {}\n", f.w, f.h, f.ptr, if f.rgb { "RGB" } else { "BGR" });
        }
        None => log!("nessun framebuffer: solo UART\n"),
    }
    let fonts = ui::Fonts {
        ui: gfx::Font::load(gfx::FONT_BIN),
        bold: gfx::Font::load(gfx::FONTB_BIN),
        large: gfx::Font::load(gfx::FONTL_BIN),
        mono: gfx::Font::load(gfx::MONO_BIN),
    };
    let model_bytes = model_region.map(|(s, e)| e - s).unwrap_or(0);
    unsafe {
        hw::BODY = Some(hw::Body { rev, serial: plat::board_serial(), total_mem: total, arm_mem: arm, fb: fb.as_ref().map(|f| (f.w, f.h)).unwrap_or((0, 0)), dt_model, model_bytes });
    }
    if let Some(f) = fb {
        let canvas = gfx::Canvas::new(f.w, f.h);
        unsafe { SCREEN = Some(Screen { fb: f, canvas, fonts }) };
    }

    // nascita: impronta del corpo
    let fp = hw::fingerprint();
    if let Some(sc) = screen() {
        let l1 = format!("{} · {} · {}", plat::model_name(rev), plat::cpu_name(), if total >= 1 << 30 { format!("{} GB", total >> 30) } else { format!("{} MB", total >> 20) });
        let l2 = format!("impronta {:016x} · kernel-ai {}", fp, VERSION);
        ui::splash(&mut sc.canvas, &sc.fonts, fp, &l1, &l2);
        sc.canvas.present(&sc.fb, 0, 0, 0, 0);
        plat::delay_ms(1500);
    }

    let mut sys = interp::Sys {
        desk: ui::Desk::new(),
        models: bais_model::manager::Manager::new(hend.saturating_sub(hstart) / 2),
        history: Vec::new(),
        diary: Vec::new(),
        habits: Vec::new(),
        pending: None,
        last_tps: None,
    };
    if let Some((s, e)) = model_region {
        let buf: &'static [u8] = unsafe { core::slice::from_raw_parts(s as *const u8, e - s) };
        match sys.models.register(buf) {
            Ok(_) => log!("modello registrato ({} KB)\n", (e - s) >> 10),
            Err(er) => log!("modello non valido: {}\n", er),
        }
    }
    sys.desk.say(&format!("B-AI-S kernel-ai {} su {}", VERSION, plat::model_name(rev)), ui::LineKind::Note);
    sys.desk.say("Mi sono svegliato in un corpo ARM. Scrivi «hardware» per sapere cosa vedo, «aiuto» per i comandi.", ui::LineKind::Note);
    if sys.models.list().count() > 0 {
        sys.desk.say("C'è un modello nell'initramfs: «modelli» e «carica 1».", ui::LineKind::Note);
    }
    refresh_sys(&mut sys);
    redraw(&mut sys);
    log!("pronto. scrivi una richiesta e premi invio.\n> ");

    // ciclo principale: UART (tastiera via seriale) + aggiornamento periodico
    let mut line: Vec<u8> = Vec::new();
    let mut esc = 0u8;
    let mut last_tick = plat::millis();
    loop {
        let mut changed = false;
        while let Some(b) = plat::uart_getb() {
            if esc > 0 {
                // sequenze ANSI (frecce ecc.): ignorate
                if esc == 1 && b == b'[' {
                    esc = 2;
                } else if (0x40..=0x7e).contains(&b) {
                    esc = 0;
                }
                continue;
            }
            match b {
                0x1b => esc = 1,
                b'\r' | b'\n' => {
                    plat::uart_puts("\n");
                    let s = String::from_utf8_lossy(&line).into_owned();
                    line.clear();
                    sys.desk.input.clear();
                    let mut rd = |st: &mut interp::Sys| redraw(st);
                    sys.handle(&s, &mut rd);
                    refresh_sys(&mut sys);
                    redraw(&mut sys);
                    plat::uart_puts("> ");
                }
                0x7f | 0x08 => {
                    // cancella un carattere UTF-8 intero
                    while let Some(c) = line.pop() {
                        if c & 0xC0 != 0x80 {
                            break;
                        }
                    }
                    plat::uart_puts("\x08 \x08");
                    changed = true;
                }
                0x03 => {
                    line.clear();
                    plat::uart_puts("^C\n> ");
                    changed = true;
                }
                b if b >= 0x20 || b >= 0x80 => {
                    line.push(b);
                    plat::uart_putb(b);
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            sys.desk.input = String::from_utf8_lossy(&line).into_owned();
            redraw_prompt(&sys);
        }
        let now = plat::millis();
        if now - last_tick >= 5000 {
            last_tick = now;
            refresh_sys(&mut sys);
            redraw(&mut sys);
        }
    }
}

fn fatal(title: &str, lines: &[String]) -> ! {
    log!("\n*** {} ***\n", title);
    for l in lines {
        log!("{}\n", l);
    }
    if let Some(sc) = screen() {
        ui::fatal(&mut sc.canvas, &sc.fonts, title, lines);
        sc.canvas.present(&sc.fb, 0, 0, 0, 0);
    }
    loop {
        unsafe { core::arch::asm!("wfe") };
    }
}

#[no_mangle]
pub extern "C" fn exception(kind: u64, esr: u64, elr: u64, far: u64) -> ! {
    let ec = (esr >> 26) & 0x3f;
    let what = match ec {
        0x15 => "chiamata di sistema",
        0x20 | 0x21 => "errore nel prelievo di istruzioni",
        0x24 | 0x25 => "accesso ai dati non valido",
        0x22 => "PC non allineato",
        0x26 => "stack non allineato",
        0x00 => "istruzione sconosciuta",
        _ => "eccezione",
    };
    let src = ["sincrona", "IRQ", "FIQ", "SError"][(kind & 3) as usize];
    fatal(
        "Eccezione della CPU",
        &[
            format!("{} ({}), vettore {}", what, src, kind),
            format!("ESR {:#018x}   classe {:#04x}", esr, ec),
            format!("ELR {:#018x}   (indirizzo dell'istruzione)", elr),
            format!("FAR {:#018x}   (indirizzo dei dati)", far),
        ],
    )
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let mut lines = Vec::new();
    if let Some(l) = info.location() {
        lines.push(format!("{}:{}", l.file(), l.line()));
    }
    lines.push(format!("{}", info.message()));
    fatal("Panic del kernel", &lines)
}
