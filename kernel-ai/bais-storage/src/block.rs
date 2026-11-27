use crate::{Error, Result};

/// Il contratto con un driver di memoria di massa (USB mass storage, AHCI,
/// NVMe o, in UEFI, il protocollo BlockIo del firmware).
///
/// Le letture sono in blocchi interi: `buf.len()` è sempre un multiplo di
/// `block_size()`. Il driver può leggere più blocchi contigui in una sola
/// richiesta: è la via veloce per i file di modello da molti GB.
pub trait BlockDevice {
    /// Byte per blocco (512 sulle chiavette, 512 o 4096 sui dischi).
    fn block_size(&self) -> u32;
    /// Numero totale di blocchi.
    fn num_blocks(&self) -> u64;
    /// Legge `buf.len() / block_size()` blocchi a partire da `lba`.
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<()>;
}

impl<D: BlockDevice + ?Sized> BlockDevice for &mut D {
    fn block_size(&self) -> u32 {
        (**self).block_size()
    }
    fn num_blocks(&self) -> u64 {
        (**self).num_blocks()
    }
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<()> {
        (**self).read_blocks(lba, buf)
    }
}

/// Controlla che la dimensione di blocco sia gestibile.
pub fn check_block_size(bs: u32) -> Result<usize> {
    if (512..=crate::MAX_SECTOR as u32).contains(&bs) && bs.is_power_of_two() {
        Ok(bs as usize)
    } else {
        Err(Error::BadBlockSize)
    }
}

/// Vista su una parte del dispositivo: il blocco 0 della vista è `start`.
/// Rifiuta le letture che escono dalla partizione.
pub struct Partition<D> {
    dev: D,
    start: u64,
    len: u64,
}

impl<D: BlockDevice> Partition<D> {
    pub fn new(dev: D, start: u64, len: u64) -> Self {
        Partition { dev, start, len }
    }
    pub fn start(&self) -> u64 {
        self.start
    }
    pub fn into_inner(self) -> D {
        self.dev
    }
}

impl<D: BlockDevice> BlockDevice for Partition<D> {
    fn block_size(&self) -> u32 {
        self.dev.block_size()
    }
    fn num_blocks(&self) -> u64 {
        self.len
    }
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<()> {
        let bs = self.dev.block_size() as u64;
        if buf.len() as u64 % bs != 0 {
            return Err(Error::BadBlockSize);
        }
        let n = buf.len() as u64 / bs;
        let end = lba.checked_add(n).ok_or(Error::OutOfRange)?;
        if end > self.len {
            return Err(Error::OutOfRange);
        }
        self.dev.read_blocks(self.start + lba, buf)
    }
}

#[cfg(test)]
pub(crate) mod testdev {
    use super::*;
    use std::vec::Vec;

    /// Dispositivo in memoria per i test, con contatore delle richieste.
    pub struct MemDev {
        pub data: Vec<u8>,
        pub bs: u32,
        pub reads: usize,
    }

    impl BlockDevice for MemDev {
        fn block_size(&self) -> u32 {
            self.bs
        }
        fn num_blocks(&self) -> u64 {
            (self.data.len() / self.bs as usize) as u64
        }
        fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<()> {
            self.reads += 1;
            let o = lba as usize * self.bs as usize;
            if o + buf.len() > self.data.len() {
                return Err(Error::OutOfRange);
            }
            buf.copy_from_slice(&self.data[o..o + buf.len()]);
            Ok(())
        }
    }

    #[test]
    fn partition_bounds() {
        let mut d = MemDev { data: std::vec![0u8; 512 * 10], bs: 512, reads: 0 };
        let mut p = Partition::new(&mut d, 2, 4);
        let mut b = [0u8; 1024];
        assert!(p.read_blocks(2, &mut b).is_ok());
        assert_eq!(p.read_blocks(3, &mut b), Err(Error::OutOfRange));
        assert_eq!(p.read_blocks(u64::MAX, &mut b), Err(Error::OutOfRange));
    }
}
