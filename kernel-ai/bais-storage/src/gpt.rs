//! Tabelle delle partizioni: GPT (intestazione e voci verificate con CRC32,
//! ripiego sulla copia di riserva in fondo al disco) e MBR classico.

use crate::block::check_block_size;
use crate::crc32::Crc32;
use crate::{le32, le64, BlockDevice, Error, Result, MAX_SECTOR};

/// GUID «Microsoft basic data» (EBD0A0A2-B9E5-4433-87C0-68B6B72699C7), quello
/// che Windows usa per exFAT/NTFS/FAT, già nell'ordine dei byte su disco.
pub const GUID_BASIC_DATA: [u8; 16] =
    [0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7];
/// GUID della partizione di sistema EFI (C12A7328-F81F-11D2-BA4B-00A0C93EC93B).
pub const GUID_EFI_SYSTEM: [u8; 16] =
    [0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Gpt,
    Mbr,
}

#[derive(Debug, Clone, Copy)]
pub struct PartInfo {
    pub scheme: Scheme,
    /// Posizione nella tabella, da 0.
    pub index: u32,
    pub first_lba: u64,
    /// Ultimo blocco, incluso.
    pub last_lba: u64,
    /// Solo GPT.
    pub type_guid: [u8; 16],
    /// Solo MBR (0x07 = exFAT/NTFS).
    pub mbr_type: u8,
    /// Nome UTF-16 (solo GPT), terminato da 0.
    pub name: [u16; 36],
}

impl PartInfo {
    pub fn num_blocks(&self) -> u64 {
        self.last_lba - self.first_lba + 1
    }
}

struct Header {
    entries_lba: u64,
    count: u32,
    size: u32,
    entries_crc: u32,
}

fn read_header<D: BlockDevice>(dev: &mut D, lba: u64, sec: &mut [u8]) -> Result<Option<Header>> {
    let bs = sec.len();
    dev.read_blocks(lba, sec)?;
    if &sec[0..8] != b"EFI PART" {
        return Ok(None);
    }
    let hsize = le32(sec, 12) as usize;
    if !(92..=bs).contains(&hsize) {
        return Ok(None);
    }
    let stored = le32(sec, 16);
    let mut c = Crc32::new();
    c.update(&sec[0..16]);
    c.update(&[0, 0, 0, 0]);
    c.update(&sec[20..hsize]);
    if c.finish() != stored || le64(sec, 24) != lba {
        return Ok(None);
    }
    let h = Header {
        entries_lba: le64(sec, 72),
        count: le32(sec, 80),
        size: le32(sec, 84),
        entries_crc: le32(sec, 88),
    };
    // Voci di almeno 128 byte, potenza di 2, mai a cavallo di due blocchi.
    if h.size < 128 || !h.size.is_power_of_two() || h.size as usize > bs || h.count == 0 || h.count > 1024 {
        return Ok(None);
    }
    Ok(Some(h))
}

fn entries_crc_ok<D: BlockDevice>(dev: &mut D, h: &Header, sec: &mut [u8]) -> Result<bool> {
    let bs = sec.len();
    let mut left = h.count as usize * h.size as usize;
    let mut lba = h.entries_lba;
    let mut c = Crc32::new();
    while left > 0 {
        dev.read_blocks(lba, sec)?;
        let n = left.min(bs);
        c.update(&sec[..n]);
        left -= n;
        lba += 1;
    }
    Ok(c.finish() == h.entries_crc)
}

/// Chiama `f` per ogni partizione non vuota; `f` restituisce `false` per
/// fermarsi. Restituisce lo schema trovato. Le voci GPT vengono passate solo
/// dopo aver verificato il CRC32 dell'intera tabella.
pub fn for_each_partition<D, F>(dev: &mut D, mut f: F) -> Result<Scheme>
where
    D: BlockDevice,
    F: FnMut(&mut D, &PartInfo) -> bool,
{
    let bs = check_block_size(dev.block_size())?;
    let mut secbuf = [0u8; MAX_SECTOR];
    let sec = &mut secbuf[..bs];
    let total = dev.num_blocks();
    if total < 2 {
        return Err(Error::NoPartitionTable);
    }

    dev.read_blocks(0, sec)?;
    if sec[510] != 0x55 || sec[511] != 0xAA {
        return Err(Error::NoPartitionTable);
    }
    // Un volume senza tabella («superfloppy») ha anche lui 55 AA: lì i byte
    // 446..510 sono codice di avvio, non partizioni.
    if &sec[3..11] == b"EXFAT   " || &sec[3..11] == b"NTFS    " || &sec[82..90] == b"FAT32   " {
        return Err(Error::NoPartitionTable);
    }
    let mut mbr = [(0u8, 0u32, 0u32); 4];
    let mut protective = false;
    for (i, m) in mbr.iter_mut().enumerate() {
        let o = 446 + 16 * i;
        *m = (sec[o + 4], le32(sec, o + 8), le32(sec, o + 12));
        if m.0 == 0xEE {
            protective = true;
        }
    }

    if protective {
        let mut hdr = read_header(dev, 1, sec)?;
        let mut ok = false;
        if let Some(h) = &hdr {
            ok = entries_crc_ok(dev, h, sec)?;
        }
        if !ok {
            // Copia di riserva nell'ultimo blocco.
            hdr = read_header(dev, total - 1, sec)?;
            if let Some(h) = &hdr {
                ok = entries_crc_ok(dev, h, sec)?;
            }
        }
        let h = match (ok, hdr) {
            (true, Some(h)) => h,
            _ => return Err(Error::GptCrc),
        };
        let per = bs / h.size as usize;
        let mut idx = 0u32;
        let mut lba = h.entries_lba;
        while idx < h.count {
            dev.read_blocks(lba, sec)?;
            for k in 0..per {
                if idx >= h.count {
                    break;
                }
                let e = &sec[k * h.size as usize..];
                let mut ty = [0u8; 16];
                ty.copy_from_slice(&e[0..16]);
                let first = le64(e, 32);
                let last = le64(e, 40);
                if ty != [0u8; 16] && last >= first && last < total {
                    let mut name = [0u16; 36];
                    for (j, n) in name.iter_mut().enumerate() {
                        *n = u16::from_le_bytes([e[56 + 2 * j], e[57 + 2 * j]]);
                    }
                    let p = PartInfo {
                        scheme: Scheme::Gpt,
                        index: idx,
                        first_lba: first,
                        last_lba: last,
                        type_guid: ty,
                        mbr_type: 0,
                        name,
                    };
                    if !f(dev, &p) {
                        return Ok(Scheme::Gpt);
                    }
                }
                idx += 1;
            }
            lba += 1;
        }
        return Ok(Scheme::Gpt);
    }

    // MBR classico: solo le 4 primarie (le logiche non servono per la chiavetta).
    let mut any = false;
    for (i, &(ty, start, count)) in mbr.iter().enumerate() {
        if ty == 0 || ty == 0x05 || ty == 0x0F || count == 0 {
            continue;
        }
        let first = start as u64;
        let last = first + count as u64 - 1;
        if last >= total {
            continue;
        }
        any = true;
        let p = PartInfo {
            scheme: Scheme::Mbr,
            index: i as u32,
            first_lba: first,
            last_lba: last,
            type_guid: [0; 16],
            mbr_type: ty,
            name: [0; 36],
        };
        if !f(dev, &p) {
            break;
        }
    }
    if any {
        Ok(Scheme::Mbr)
    } else {
        Err(Error::NoPartitionTable)
    }
}
