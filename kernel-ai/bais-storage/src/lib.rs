//! # bais-storage
//!
//! Pezzo della Fase 4 di kernel-ai (B-AI-S): leggere dalla chiavetta i file
//! di modello da molti GB.
//!
//! - [`block`]: il contratto con il driver (`BlockDevice`) e la vista su una partizione.
//! - [`gpt`]: tabella delle partizioni GPT (con verifica CRC32) e MBR.
//! - [`exfat`]: file system exFAT in sola lettura, file oltre i 4 GB.
//! - [`sha256`]: SHA-256 a flusso, per verificare i pesi mentre si leggono.
//!
//! `no_std`, nessuna allocazione dinamica, nessuna dipendenza: entra nel
//! kernel così com'è e gira anche dentro un'applicazione UEFI.
#![no_std]
#![forbid(unsafe_code)]
// `%` invece di `is_multiple_of`: il kernel può usare un Rust più vecchio.
#![allow(clippy::manual_is_multiple_of, clippy::large_enum_variant)]

#[cfg(test)]
extern crate std;

pub mod block;
pub mod crc32;
pub mod error;
pub mod exfat;
pub mod gpt;
pub mod sha256;

pub use block::{BlockDevice, Partition};
pub use error::{Error, Result};

/// Dimensione massima del settore gestita (exFAT ammette 512..4096 byte).
pub const MAX_SECTOR: usize = 4096;

#[inline]
pub(crate) fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
#[inline]
pub(crate) fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
#[inline]
pub(crate) fn le64(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}

/// Trova il primo volume exFAT sul dispositivo: prima nella GPT, poi
/// nell'MBR, infine sul disco intero senza tabella («superfloppy»).
/// Restituisce primo blocco e numero di blocchi del volume.
pub fn find_exfat<D: BlockDevice>(dev: &mut D) -> Result<(u64, u64)> {
    let mut found: Option<(u64, u64)> = None;
    let mut scan = |dev: &mut D, start: u64, len: u64| -> bool {
        let mut p = Partition::new(&mut *dev, start, len);
        if exfat::probe(&mut p).unwrap_or(false) {
            found = Some((start, len));
            return false; // ferma la scansione
        }
        true
    };
    match gpt::for_each_partition(dev, |d, part| scan(d, part.first_lba, part.num_blocks())) {
        Ok(_) => {}
        Err(Error::NoPartitionTable) => {}
        Err(e) => return Err(e),
    }
    if let Some(f) = found {
        return Ok(f);
    }
    let total = dev.num_blocks();
    let mut p = Partition::new(&mut *dev, 0, total);
    if exfat::probe(&mut p)? {
        return Ok((0, total));
    }
    Err(Error::NoExfat)
}
