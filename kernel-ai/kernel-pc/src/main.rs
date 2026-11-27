//! kernel-ai di B-AI-S per PC x86_64 — prototipo v0.3-pc.
//!
//! Stesso sistema della v0.3-rpi (desktop che si modifica da frase, diario, abitudini,
//! brain in RAM, splash di nascita dal corpo, gestore modelli `bais-model`), avviato da GRUB
//! (multiboot2, BIOS). Si scrive dalla tastiera PS/2 (layout italiano) o dalla seriale COM1.
//! Il modello di prova arriva come modulo multiboot2.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::global_asm;

mod gfx;
mod heap;
mod hw;
mod interp;
mod mem;
mod nlu;
mod plat;
mod ui;

global_asm!(include_str!("boot.s"), options(att_syntax));

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

const VERSION: &str = "0.3.0-pc";
const MB2_MAGIC: u32 = 0x36d7_6289;

fn refresh_sys(s: &mut interp::Sys) {
    s.desk.sys = hw::sys_lines(s);
    s.desk.tray = match plat::rtc_time() {
        Some((h, m, _)) => format!("{:02}:{:02}", h, m),
        None => {
            let up = plat::millis() / 1000;
            format!("{:02}:{:02}:{:02}", up / 3600, (up / 60) % 60, up % 60)
        }
    };
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
pub extern "C" fn kmain(magic: u32, info: usize) -> ! {
    plat::uart_init();
    plat::idt_init();
    log!("\n\nB-AI-S kernel-ai {} — PC x86_64\n", VERSION);
    if magic != MB2_MAGIC {
        log!("avviato senza multiboot2 (magic {:#x}): mi fermo\n", magic);
        plat::halt();
    }
    plat::timer_init();
    log!("TSC ~{} MHz\n", plat::tsc_mhz());

    // heap: dopo il kernel, i moduli e le info di GRUB, fino alla fine della RAM sotto i 4 GiB
    let kend = core::ptr::addr_of!(__kernel_end) as usize;
    let hstart = kend.max(plat::mb2_reserved_end(info)).max(0x20_0000);
    let mut hend = plat::mb2_ram_limit(info, hstart);
    if hend <= hstart + (16 << 20) {
        hend = hstart + (16 << 20); // ripiego prudente
    }
    // il framebuffer sta di solito sopra la RAM; in ogni caso resta fuori dall'heap
    heap::init(hstart, hend);
    log!("heap {:#x}..{:#x} ({} MB)\n", hstart, hend, (hend - hstart) >> 20);

    let bi = plat::mb2_parse(info);
    log!("avviato da «{}», RAM {} MB, {} moduli\n", bi.loader, bi.ram_total >> 20, bi.modules.len());
    plat::kbd_init();

    match &bi.fb {
        Some(f) => log!("framebuffer {}x{} a {:p}, ordine {}\n", f.w, f.h, f.ptr, if f.rgb { "RGB" } else { "BGR" }),
        None => log!("nessun framebuffer grafico: solo seriale\n"),
    }
    let fonts = ui::Fonts {
        ui: gfx::Font::load(gfx::FONT_BIN),
        bold: gfx::Font::load(gfx::FONTB_BIN),
        large: gfx::Font::load(gfx::FONTL_BIN),
        mono: gfx::Font::load(gfx::MONO_BIN),
    };

    let smb = plat::smbios();
    let feats = plat::features();
    let model_bytes: usize = bi.modules.iter().map(|m| m.1 - m.0).sum();
    let system: String = smb.as_ref().map(|s| { let p = s.product.split(" (").next().unwrap_or(""); String::from(format!("{} {}", s.vendor, p).trim()) }).unwrap_or_default();
    let cpu = plat::cpu_name();
    unsafe {
        hw::BODY = Some(hw::Body {
            cpu: cpu.clone(),
            vendor: plat::cpu_vendor(),
            signature: plat::cpu_signature(),
            threads: plat::cpu_threads(),
            mhz: plat::tsc_mhz(),
            ram_total: bi.ram_total,
            heap_bytes: hend - hstart,
            fb: bi.fb.as_ref().map(|f| (f.w, f.h)).unwrap_or((0, 0)),
            system,
            uuid: smb.as_ref().map(|s| s.uuid).unwrap_or([0; 16]),
            loader: bi.loader.clone(),
            model_bytes,
            emulated: feats.hypervisor || cpu.contains("Bochs") || cpu.contains("QEMU") || bi.ram_total < (2u64 << 30),
        });
    }
    let modules = bi.modules;
    if let Some(f) = bi.fb {
        let canvas = gfx::Canvas::new(f.w, f.h);
        unsafe { SCREEN = Some(Screen { fb: f, canvas, fonts }) };
    }

    // nascita: impronta del corpo
    let fp = hw::fingerprint();
    log!("impronta del corpo {:016x}\n", fp);
    if let Some(sc) = screen() {
        let ram = bi.ram_total;
        let mem = if ram >= 1 << 30 { format!("{:.1} GB", ram as f64 / (1u64 << 30) as f64) } else { format!("{} MB", ram >> 20) };
        let l1 = format!("{} · {} · {}", hw::machine_name(), cpu, mem);
        let l2 = format!("impronta {:016x} · kernel-ai {}", fp, VERSION);
        ui::splash(&mut sc.canvas, &sc.fonts, fp, &l1, &l2);
        sc.canvas.present(&sc.fb, 0, 0, 0, 0);
        plat::delay_ms(1500);
    }

    let mut sys = interp::Sys {
        desk: ui::Desk::new(),
        models: bais_model::manager::Manager::new((hend - hstart) / 2),
        history: Vec::new(),
        diary: Vec::new(),
        habits: Vec::new(),
        pending: None,
        last_tps: None,
    };
    for (s, e, name) in &modules {
        let buf: &'static [u8] = unsafe { core::slice::from_raw_parts(*s as *const u8, e - s) };
        if buf.len() < 4 || &buf[..4] != b"GGUF" {
            log!("modulo «{}» ignorato: non è un GGUF\n", name);
            continue;
        }
        match sys.models.register(buf) {
            Ok(id) => {
                log!("modello «{}» registrato ({} KB)\n", name, (e - s) >> 10);
                // il classificatore degli intenti è piccolo: lo scheduler lo vuole sempre pronto
                if sys.models.list().nth(id).map(|i| i.caps.classify).unwrap_or(false) {
                    match sys.models.load(id) {
                        Ok(()) => log!("classificatore «{}» caricato: lo scheduler lo usa per le frasi sul tema\n", name),
                        Err(er) => log!("classificatore «{}» non caricato: {}\n", name, er),
                    }
                }
            }
            Err(er) => log!("modello «{}» non valido: {}\n", name, er),
        }
    }
    sys.desk.say(&format!("B-AI-S kernel-ai {} su {}", VERSION, hw::machine_name()), ui::LineKind::Note);
    sys.desk.say("Mi sono svegliato in un corpo x86. Scrivi «hardware» per sapere cosa vedo, «aiuto» per i comandi.", ui::LineKind::Note);
    if sys.models.list().any(|i| i.caps.classify && i.status != bais_model::manager::Status::Registered) {
        sys.desk.say("Le frasi sul tema le capisce il classificatore «bais-intent»: prova «vorrei un desktop tipo mac al buio».", ui::LineKind::Note);
    }
    if sys.models.list().any(|i| !i.caps.classify) {
        sys.desk.say("C'è un modello di prova caricato da GRUB: «modelli», «carica 1», poi «genera ciao».", ui::LineKind::Note);
    }
    refresh_sys(&mut sys);
    redraw(&mut sys);
    log!("pronto. scrivi una richiesta e premi invio.\n> ");

    // ciclo principale: tastiera PS/2 + seriale, aggiornamento periodico dell'orologio
    let mut line: Vec<u8> = Vec::new();
    let mut esc = 0u8;
    let mut last_tick = plat::millis();
    // il campo di richiesta si ridisegna solo quando la digitazione fa una pausa:
    // in un emulatore lento ridisegnarlo a ogni tasto fa traboccare il buffer della tastiera
    let mut dirty = false;
    let mut last_key = 0u64;
    loop {
        let mut changed = false;
        loop {
            let b = match plat::kbd_getb().or_else(plat::uart_getb) {
                Some(b) => b,
                None => break,
            };
            if esc > 0 {
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
                    dirty = false;
                    plat::uart_puts("> ");
                }
                0x7f | 0x08 => {
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
                b if b >= 0x20 => {
                    line.push(b);
                    plat::uart_putb(b);
                    changed = true;
                }
                _ => {}
            }
        }
        if changed {
            dirty = true;
            last_key = plat::millis();
        }
        if dirty && plat::millis() - last_key >= 60 {
            dirty = false;
            sys.desk.input = String::from_utf8_lossy(&line).into_owned();
            redraw_prompt(&sys);
        }
        // l'orologio mostra le ore:minuti: basta ridisegnare quando cambia il minuto
        let now = plat::millis();
        if now - last_tick >= 20_000 {
            last_tick = now;
            let old = sys.desk.tray.clone();
            refresh_sys(&mut sys);
            if sys.desk.tray != old {
                redraw(&mut sys);
            }
        }
        core::hint::spin_loop();
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
    plat::halt()
}

#[no_mangle]
pub extern "C" fn exception(vector: u64, code: u64, rip: u64, cr2: u64) -> ! {
    let what = match vector {
        0 => "divisione per zero",
        3 => "breakpoint",
        6 => "istruzione non valida",
        8 => "doppio errore",
        13 => "protezione generale",
        14 => "errore di pagina",
        16 | 19 => "errore in virgola mobile",
        _ => "eccezione",
    };
    fatal(
        "Eccezione della CPU",
        &[
            format!("{} (vettore {})", what, vector),
            format!("codice d'errore {:#x}", code),
            format!("RIP {:#018x}   (indirizzo dell'istruzione)", rip),
            format!("CR2 {:#018x}   (indirizzo dei dati, per gli errori di pagina)", cr2),
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
