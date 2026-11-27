//! Lettore minimo del device tree passato dal firmware: modello della scheda
//! e posizione dell'initramfs (`/chosen/linux,initrd-start|end`), dove il firmware
//! mette il file del modello (`initramfs bais-model.gguf followkernel` in config.txt).

pub struct Info {
    pub model: [u8; 64],
    pub model_len: usize,
    pub initrd: Option<(usize, usize)>,
}

fn be32(p: *const u8) -> u32 {
    unsafe { u32::from_be_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]) }
}

fn cell(p: *const u8, len: usize) -> usize {
    if len >= 8 {
        ((be32(p) as usize) << 32) | be32(unsafe { p.add(4) }) as usize
    } else {
        be32(p) as usize
    }
}

unsafe fn cstr<'a>(p: *const u8) -> &'a [u8] {
    let mut n = 0;
    while *p.add(n) != 0 && n < 256 {
        n += 1;
    }
    core::slice::from_raw_parts(p, n)
}

pub fn parse(dtb: usize) -> Option<Info> {
    if dtb == 0 || dtb & 3 != 0 {
        return None;
    }
    let p = dtb as *const u8;
    if be32(p) != 0xd00d_feed {
        return None;
    }
    let total = be32(unsafe { p.add(4) }) as usize;
    let off_struct = be32(unsafe { p.add(8) }) as usize;
    let off_strings = be32(unsafe { p.add(12) }) as usize;
    if total > 4 << 20 {
        return None;
    }
    let mut info = Info { model: [0; 64], model_len: 0, initrd: None };
    let (mut start, mut end) = (None, None);
    let mut depth = 0usize;
    let mut in_chosen = false;
    let mut o = off_struct;
    unsafe {
        while o + 4 <= total {
            let tok = be32(p.add(o));
            o += 4;
            match tok {
                1 => {
                    let name = cstr(p.add(o));
                    o = (o + name.len() + 1 + 3) & !3;
                    depth += 1;
                    in_chosen = depth == 2 && name == b"chosen";
                }
                2 => {
                    depth = depth.saturating_sub(1);
                    in_chosen = false;
                }
                3 => {
                    let len = be32(p.add(o)) as usize;
                    let nameoff = be32(p.add(o + 4)) as usize;
                    let val = p.add(o + 8);
                    let name = cstr(p.add(off_strings + nameoff));
                    if depth == 1 && name == b"model" {
                        let v = core::slice::from_raw_parts(val, len.min(63));
                        let l = v.iter().position(|&c| c == 0).unwrap_or(v.len());
                        info.model[..l].copy_from_slice(&v[..l]);
                        info.model_len = l;
                    }
                    if in_chosen && name == b"linux,initrd-start" {
                        start = Some(cell(val, len));
                    }
                    if in_chosen && name == b"linux,initrd-end" {
                        end = Some(cell(val, len));
                    }
                    o = (o + 8 + len + 3) & !3;
                }
                4 => {}
                9 => break,
                _ => break,
            }
        }
    }
    if let (Some(s), Some(e)) = (start, end) {
        if e > s {
            info.initrd = Some((s, e));
        }
    }
    Some(info)
}
