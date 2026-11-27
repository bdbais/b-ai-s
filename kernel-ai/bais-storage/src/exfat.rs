//! exFAT in sola lettura, secondo la specifica Microsoft (revisione 1.00).
//!
//! Perché exFAT: il file GGUF di Inkling 2 Mini è di 17,6 GB e FAT32 non
//! supera i 4 GB per file. exFAT lo leggono anche Windows, macOS e Linux, quindi
//! Bais può copiare i modelli sulla chiavetta dal suo PC.
//!
//! Cosa fa:
//! - verifica il settore di avvio e il suo checksum (settori 0..10 contro 11);
//! - segue la catena dei cluster nella FAT, oppure il blocco contiguo quando
//!   il file è marcato `NoFatChain`, con protezione dai cicli;
//! - verifica checksum e hash del nome di ogni insieme di voci;
//! - confronta i nomi senza distinguere maiuscole, con la tabella delle
//!   maiuscole del volume (verificata) per U+0000..U+052F, cioè latino, greco e
//!   cirillico; oltre, confronto esatto;
//! - legge a flusso con letture di più blocchi contigui, per i file grandi;
//! - restituisce zeri oltre `ValidDataLength`, come vuole la specifica.
//!
//! Non fa: scrittura, TexFAT (usa sempre la FAT attiva indicata da
//! `VolumeFlags`), voci di directory dei produttori (ignorate).

use crate::block::check_block_size;
use crate::{le16, le32, le64, BlockDevice, Error, Result, MAX_SECTOR};

pub const ATTR_READ_ONLY: u16 = 0x01;
pub const ATTR_HIDDEN: u16 = 0x02;
pub const ATTR_SYSTEM: u16 = 0x04;
pub const ATTR_DIRECTORY: u16 = 0x10;
pub const ATTR_ARCHIVE: u16 = 0x20;

/// Caratteri coperti dalla tabella delle maiuscole tenuta in memoria.
pub const UPCASE_LEN: usize = 0x0530;
/// Lunghezza massima di un nome exFAT in unità UTF-16.
pub const MAX_NAME: usize = 255;

const CLUSTER_END: u32 = 0xFFFF_FFFF;

/// Controlla rapidamente se il blocco 0 del dispositivo è un settore di avvio exFAT.
pub fn probe<D: BlockDevice>(dev: &mut D) -> Result<bool> {
    let bs = check_block_size(dev.block_size())?;
    if dev.num_blocks() == 0 {
        return Ok(false);
    }
    let mut b = [0u8; MAX_SECTOR];
    dev.read_blocks(0, &mut b[..bs])?;
    Ok(b[0] == 0xEB && b[1] == 0x76 && b[2] == 0x90 && &b[3..11] == b"EXFAT   " && b[510] == 0x55 && b[511] == 0xAA)
}

/// Informazioni sul volume, per il comando `disco` e per i registri.
#[derive(Debug, Clone, Copy)]
pub struct Info {
    pub bytes_per_sector: u32,
    pub cluster_bytes: u32,
    pub cluster_count: u32,
    pub volume_sectors: u64,
    pub serial: u32,
    pub revision: u16,
    pub percent_in_use: u8,
    pub label: [u16; 11],
    pub label_len: u8,
    /// Vero se la tabella delle maiuscole è stata trovata e verificata.
    pub upcase_loaded: bool,
}

/// Una voce di directory già verificata (file o cartella).
#[derive(Clone)]
pub struct DirEntry {
    pub name: [u16; MAX_NAME],
    pub name_len: u8,
    pub attributes: u16,
    pub first_cluster: u32,
    pub data_length: u64,
    pub valid_data_length: u64,
    pub no_fat_chain: bool,
}

impl DirEntry {
    pub fn is_dir(&self) -> bool {
        self.attributes & ATTR_DIRECTORY != 0
    }
    pub fn name_utf16(&self) -> &[u16] {
        &self.name[..self.name_len as usize]
    }
    /// Il nome in UTF-8 dentro `out` (servono al massimo 765 byte).
    pub fn name_utf8<'a>(&self, out: &'a mut [u8]) -> Result<&'a str> {
        let mut n = 0;
        for r in char::decode_utf16(self.name_utf16().iter().copied()) {
            let c = r.unwrap_or(char::REPLACEMENT_CHARACTER);
            let l = c.len_utf8();
            if n + l > out.len() {
                return Err(Error::NameTooLong);
            }
            c.encode_utf8(&mut out[n..n + l]);
            n += l;
        }
        // Costruito da `char` validi: è sempre UTF-8 corretto.
        core::str::from_utf8(&out[..n]).map_err(|_| Error::NameTooLong)
    }
}

impl core::fmt::Debug for DirEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut b = [0u8; 765];
        let n = self.name_utf8(&mut b).unwrap_or("?");
        f.debug_struct("DirEntry")
            .field("name", &n)
            .field("dir", &self.is_dir())
            .field("len", &self.data_length)
            .field("first_cluster", &self.first_cluster)
            .field("no_fat_chain", &self.no_fat_chain)
            .finish()
    }
}

/// Posizione di lettura dentro una catena di cluster.
#[derive(Clone, Copy, Debug)]
struct Stream {
    first: u32,
    size: u64,
    valid: u64,
    contiguous: bool,
    pos: u64,
    /// Cluster che contiene l'indice `cur_index`; 0 = da ricalcolare.
    cur: u32,
    cur_index: u64,
}

impl Stream {
    fn new(first: u32, size: u64, valid: u64, contiguous: bool) -> Self {
        Stream { first, size, valid: valid.min(size), contiguous, pos: 0, cur: 0, cur_index: 0 }
    }
}

/// Un file aperto in lettura. La posizione si sposta con `seek`.
#[derive(Clone, Copy, Debug)]
pub struct File(Stream);

impl File {
    pub fn len(&self) -> u64 {
        self.0.size
    }
    pub fn is_empty(&self) -> bool {
        self.0.size == 0
    }
    pub fn position(&self) -> u64 {
        self.0.pos
    }
    pub fn seek(&mut self, pos: u64) {
        self.0.pos = pos.min(self.0.size);
    }
}

/// Una cartella aperta: si scorre con [`Volume::read_dir`].
#[derive(Clone, Copy, Debug)]
pub struct Dir(Stream);

enum Raw {
    End,
    Skip,
    File(DirEntry),
    Upcase { checksum: u32, first: u32, len: u64 },
    Label([u16; 11], u8),
}

pub struct Volume<D: BlockDevice> {
    dev: D,
    dev_bs: usize,
    /// Blocchi del dispositivo per settore del volume.
    ratio: u64,
    sector_shift: u32,
    spc_shift: u32,
    fat_lba: u64,
    heap_offset: u64,
    cluster_count: u32,
    root_cluster: u32,
    info: Info,
    fat_buf: [u8; MAX_SECTOR],
    fat_lba_cached: u64,
    blk_buf: [u8; MAX_SECTOR],
    blk_lba_cached: u64,
    upcase: [u16; UPCASE_LEN],
    /// Falso solo durante la prima scansione della radice, quando la tabella
    /// delle maiuscole (che serve all'hash dei nomi) non è ancora caricata.
    check_names: bool,
}

impl<D: BlockDevice> Volume<D> {
    /// Apre il volume: verifica il settore di avvio e il suo checksum, poi
    /// legge dalla radice etichetta e tabella delle maiuscole.
    pub fn open(mut dev: D) -> Result<Self> {
        let dev_bs = check_block_size(dev.block_size())?;
        let mut b = [0u8; MAX_SECTOR];
        dev.read_blocks(0, &mut b[..dev_bs])?;
        if !(b[0] == 0xEB && b[1] == 0x76 && b[2] == 0x90 && &b[3..11] == b"EXFAT   ") {
            return Err(Error::NoExfat);
        }
        if b[11..64].iter().any(|&x| x != 0) || b[510] != 0x55 || b[511] != 0xAA {
            return Err(Error::BadBootSector);
        }
        let volume_length = le64(&b, 72);
        let fat_offset = le32(&b, 80) as u64;
        let fat_length = le32(&b, 84) as u64;
        let heap_offset = le32(&b, 88) as u64;
        let cluster_count = le32(&b, 92);
        let root_cluster = le32(&b, 96);
        let serial = le32(&b, 100);
        let revision = le16(&b, 104);
        let flags = le16(&b, 106);
        let sector_shift = b[108] as u32;
        let spc_shift = b[109] as u32;
        let nfats = b[110] as u64;
        let percent = b[112];

        if !(9..=12).contains(&sector_shift) || spc_shift > 25 - sector_shift || !(1..=2).contains(&nfats) {
            return Err(Error::BadBootSector);
        }
        let sector = 1usize << sector_shift;
        if sector < dev_bs || sector % dev_bs != 0 {
            return Err(Error::BadBlockSize);
        }
        let ratio = (sector / dev_bs) as u64;
        if fat_offset < 24
            || heap_offset < fat_offset + fat_length * nfats
            || cluster_count == 0
            || cluster_count > 0xFFFF_FFF5
            || ((cluster_count as u64) << spc_shift) > volume_length.saturating_sub(heap_offset)
            || fat_length * (sector as u64) < (cluster_count as u64 + 2) * 4
            || root_cluster < 2
            || root_cluster > cluster_count + 1
            || volume_length.saturating_mul(ratio) > dev.num_blocks()
        {
            return Err(Error::BadBootSector);
        }

        // Checksum della regione di avvio: settori 0..10, saltando VolumeFlags
        // (106, 107) e PercentInUse (112) del settore 0; il settore 11 lo ripete.
        let mut sum: u32 = 0;
        let mut sec = [0u8; MAX_SECTOR];
        for s in 0..11u64 {
            dev.read_blocks(s * ratio, &mut sec[..sector])?;
            for (i, &x) in sec[..sector].iter().enumerate() {
                if s == 0 && (i == 106 || i == 107 || i == 112) {
                    continue;
                }
                sum = sum.rotate_right(1).wrapping_add(x as u32);
            }
        }
        dev.read_blocks(11 * ratio, &mut sec[..sector])?;
        if sec[..sector].chunks_exact(4).any(|c| le32(c, 0) != sum) {
            return Err(Error::BootChecksum);
        }

        let active_fat = if nfats == 2 && flags & 1 != 0 { 1 } else { 0 };
        let mut upcase = [0u16; UPCASE_LEN];
        for (i, u) in upcase.iter_mut().enumerate() {
            *u = i as u16;
        }
        let mut v = Volume {
            dev,
            dev_bs,
            ratio,
            sector_shift,
            spc_shift,
            fat_lba: (fat_offset + active_fat * fat_length) * ratio,
            heap_offset,
            cluster_count,
            root_cluster,
            info: Info {
                bytes_per_sector: sector as u32,
                cluster_bytes: (sector as u32) << spc_shift,
                cluster_count,
                volume_sectors: volume_length,
                serial,
                revision,
                percent_in_use: percent,
                label: [0; 11],
                label_len: 0,
                upcase_loaded: false,
            },
            fat_buf: [0; MAX_SECTOR],
            fat_lba_cached: u64::MAX,
            blk_buf: [0; MAX_SECTOR],
            blk_lba_cached: u64::MAX,
            upcase,
            check_names: false,
        };

        // Nella radice: etichetta e tabella delle maiuscole.
        let mut root = v.root()?;
        let mut up: Option<(u32, u32, u64)> = None;
        loop {
            match v.next_raw(&mut root)? {
                Raw::End => break,
                Raw::Upcase { checksum, first, len } => up = Some((checksum, first, len)),
                Raw::Label(l, n) => {
                    v.info.label = l;
                    v.info.label_len = n;
                }
                _ => {}
            }
        }
        if let Some((checksum, first, len)) = up {
            v.load_upcase(checksum, first, len)?;
        }
        v.check_names = true;
        Ok(v)
    }

    pub fn info(&self) -> Info {
        self.info
    }

    pub fn into_inner(self) -> D {
        self.dev
    }

    fn cluster_bytes(&self) -> u64 {
        1u64 << (self.sector_shift + self.spc_shift)
    }

    /// Primo blocco del dispositivo del cluster `c`.
    fn cluster_lba(&self, c: u32) -> u64 {
        (self.heap_offset + (((c - 2) as u64) << self.spc_shift)) * self.ratio
    }

    fn valid_cluster(&self, c: u32) -> bool {
        c >= 2 && c <= self.cluster_count + 1
    }

    /// Cluster successivo nella FAT; `None` a fine catena.
    fn fat_next(&mut self, c: u32) -> Result<Option<u32>> {
        if !self.valid_cluster(c) {
            return Err(Error::BadClusterChain);
        }
        let off = c as u64 * 4;
        let lba = self.fat_lba + off / self.dev_bs as u64;
        if lba != self.fat_lba_cached {
            let bs = self.dev_bs;
            self.dev.read_blocks(lba, &mut self.fat_buf[..bs])?;
            self.fat_lba_cached = lba;
        }
        let n = le32(&self.fat_buf, (off % self.dev_bs as u64) as usize);
        if n == CLUSTER_END {
            Ok(None)
        } else if self.valid_cluster(n) {
            Ok(Some(n))
        } else {
            // 0 (libero), 0xFFFFFFF7 (difettoso) o fuori range.
            Err(Error::BadClusterChain)
        }
    }

    /// Porta lo stream sul cluster numero `ci` del file.
    fn seek_cluster(&mut self, s: &mut Stream, ci: u64) -> Result<u32> {
        if s.contiguous {
            let c = s.first as u64 + ci;
            if c > self.cluster_count as u64 + 1 || !self.valid_cluster(s.first) {
                return Err(Error::BadClusterChain);
            }
            s.cur = c as u32;
            s.cur_index = ci;
            return Ok(s.cur);
        }
        if s.cur == 0 || s.cur_index > ci {
            if !self.valid_cluster(s.first) {
                return Err(Error::BadClusterChain);
            }
            s.cur = s.first;
            s.cur_index = 0;
        }
        while s.cur_index < ci {
            // Un indice oltre il numero di cluster del volume è per forza un ciclo.
            if s.cur_index >= self.cluster_count as u64 {
                return Err(Error::BadClusterChain);
            }
            s.cur = self.fat_next(s.cur)?.ok_or(Error::BadClusterChain)?;
            s.cur_index += 1;
        }
        Ok(s.cur)
    }

    /// Quanti cluster contigui a partire da `s.cur`, fino a `max`.
    fn contiguous_run(&mut self, s: &Stream, max: u64) -> Result<u64> {
        if s.contiguous {
            return Ok(max);
        }
        let mut n = 1u64;
        let mut c = s.cur;
        while n < max {
            match self.fat_next(c)? {
                Some(x) if x == c + 1 => {
                    c = x;
                    n += 1;
                }
                _ => break,
            }
        }
        Ok(n)
    }

    fn stream_read(&mut self, s: &mut Stream, buf: &mut [u8]) -> Result<usize> {
        let cb = self.cluster_bytes();
        let bs = self.dev_bs as u64;
        let mut done = 0usize;
        while done < buf.len() && s.pos < s.size {
            let want = (buf.len() - done) as u64;
            let want = want.min(s.size - s.pos);
            if s.pos >= s.valid {
                // Oltre ValidDataLength il contenuto è indefinito su disco: zeri.
                for x in &mut buf[done..done + want as usize] {
                    *x = 0;
                }
                done += want as usize;
                s.pos += want;
                continue;
            }
            let want = want.min(s.valid - s.pos);
            let ci = s.pos / cb;
            let within = s.pos % cb;
            let cluster = self.seek_cluster(s, ci)?;
            let clusters_needed = (within + want).div_ceil(cb);
            let run = self.contiguous_run(s, clusters_needed)?;
            let avail = run * cb - within;
            let n = want.min(avail);
            let byte_lba = self.cluster_lba(cluster) * bs + within;
            let lba = byte_lba / bs;
            let off = byte_lba % bs;
            let out = &mut buf[done..];
            let got = if off == 0 && n >= bs {
                // Via veloce: blocchi interi direttamente nel buffer del chiamante.
                let whole = (n / bs) * bs;
                self.dev.read_blocks(lba, &mut out[..whole as usize])?;
                whole
            } else {
                if lba != self.blk_lba_cached {
                    let b = self.dev_bs;
                    self.dev.read_blocks(lba, &mut self.blk_buf[..b])?;
                    self.blk_lba_cached = lba;
                }
                let k = n.min(bs - off);
                out[..k as usize].copy_from_slice(&self.blk_buf[off as usize..(off + k) as usize]);
                k
            };
            done += got as usize;
            s.pos += got;
        }
        Ok(done)
    }

    /// Legge dal file nella posizione corrente; restituisce i byte letti
    /// (meno di `buf.len()` solo a fine file).
    pub fn read(&mut self, f: &mut File, buf: &mut [u8]) -> Result<usize> {
        self.stream_read(&mut f.0, buf)
    }

    /// La cartella radice.
    pub fn root(&mut self) -> Result<Dir> {
        // La radice non ha lunghezza scritta: la si ricava dalla catena.
        let mut n = 1u64;
        let mut c = self.root_cluster;
        while let Some(x) = self.fat_next(c)? {
            n += 1;
            if n > self.cluster_count as u64 {
                return Err(Error::BadClusterChain);
            }
            c = x;
        }
        let size = n * self.cluster_bytes();
        Ok(Dir(Stream::new(self.root_cluster, size, size, false)))
    }

    /// Controlla che la catena del file abbia esattamente i cluster che la sua
    /// lunghezza richiede e termini con la marca di fine. Così un ciclo o una
    /// catena troncata vengono scoperti all'apertura, prima di restituire dati
    /// sbagliati (un ciclo dentro la lunghezza del file altrimenti ripeterebbe
    /// dei cluster senza errori).
    fn check_chain(&mut self, first: u32, size: u64, contiguous: bool) -> Result<()> {
        if size == 0 {
            return Ok(());
        }
        let n = size.div_ceil(self.cluster_bytes());
        if !self.valid_cluster(first) || n > self.cluster_count as u64 {
            return Err(Error::BadClusterChain);
        }
        if contiguous {
            return if first as u64 + n - 1 <= self.cluster_count as u64 + 1 { Ok(()) } else { Err(Error::BadClusterChain) };
        }
        let mut c = first;
        for _ in 1..n {
            c = self.fat_next(c)?.ok_or(Error::BadClusterChain)?;
        }
        match self.fat_next(c)? {
            None => Ok(()),
            Some(_) => Err(Error::BadClusterChain),
        }
    }

    pub fn open_dir(&mut self, e: &DirEntry) -> Result<Dir> {
        if !e.is_dir() {
            return Err(Error::NotADirectory);
        }
        self.check_chain(e.first_cluster, e.data_length, e.no_fat_chain)?;
        Ok(Dir(Stream::new(e.first_cluster, e.data_length, e.data_length, e.no_fat_chain)))
    }

    pub fn open_file(&mut self, e: &DirEntry) -> Result<File> {
        if e.is_dir() {
            return Err(Error::IsADirectory);
        }
        if e.data_length > 0 && !self.valid_cluster(e.first_cluster) {
            return Err(Error::BadDirEntry);
        }
        self.check_chain(e.first_cluster, e.data_length, e.no_fat_chain)?;
        Ok(File(Stream::new(e.first_cluster, e.data_length, e.valid_data_length, e.no_fat_chain)))
    }

    fn read_entry(&mut self, d: &mut Dir, e: &mut [u8; 32]) -> Result<bool> {
        let n = self.stream_read(&mut d.0, e)?;
        Ok(n == 32)
    }

    fn next_raw(&mut self, d: &mut Dir) -> Result<Raw> {
        let mut e = [0u8; 32];
        loop {
            if !self.read_entry(d, &mut e)? {
                return Ok(Raw::End);
            }
            let ty = e[0];
            if ty == 0x00 {
                // Fine della directory: le voci successive non vanno lette.
                d.0.pos = d.0.size;
                return Ok(Raw::End);
            }
            if ty & 0x80 == 0 {
                continue; // voce cancellata o libera
            }
            return match ty {
                0x85 => self.parse_file_set(d, &e),
                0x82 => Ok(Raw::Upcase { checksum: le32(&e, 4), first: le32(&e, 20), len: le64(&e, 24) }),
                0x83 => {
                    let n = (e[1] as usize).min(11);
                    let mut l = [0u16; 11];
                    for (i, x) in l.iter_mut().enumerate().take(n) {
                        *x = le16(&e, 2 + 2 * i);
                    }
                    Ok(Raw::Label(l, n as u8))
                }
                _ => Ok(Raw::Skip),
            };
        }
    }

    fn parse_file_set(&mut self, d: &mut Dir, first: &[u8; 32]) -> Result<Raw> {
        let sc = first[1] as usize;
        if !(2..=18).contains(&sc) {
            return Err(Error::BadDirEntry);
        }
        let mut set = [[0u8; 32]; 19];
        set[0] = *first;
        for item in set.iter_mut().take(sc + 1).skip(1) {
            if !self.read_entry(d, item)? {
                return Err(Error::BadDirEntry);
            }
            if item[0] & 0x80 == 0 {
                return Err(Error::BadDirEntry);
            }
        }
        // Checksum dell'insieme, saltando il campo SetChecksum (byte 2..3).
        let mut sum: u16 = 0;
        for (k, ent) in set.iter().take(sc + 1).enumerate() {
            for (i, &x) in ent.iter().enumerate() {
                if k == 0 && (i == 2 || i == 3) {
                    continue;
                }
                sum = sum.rotate_right(1).wrapping_add(x as u16);
            }
        }
        if sum != le16(first, 2) {
            return Err(Error::BadDirEntry);
        }
        let st = &set[1];
        if st[0] != 0xC0 {
            return Err(Error::BadDirEntry);
        }
        let name_len = st[3] as usize;
        let name_entries = name_len.div_ceil(15);
        if name_len == 0 || name_entries > sc - 1 {
            return Err(Error::BadDirEntry);
        }
        let mut ent = DirEntry {
            name: [0; MAX_NAME],
            name_len: name_len as u8,
            attributes: le16(first, 4),
            first_cluster: le32(st, 20),
            data_length: le64(st, 24),
            valid_data_length: le64(st, 8),
            no_fat_chain: st[1] & 0x02 != 0,
        };
        for k in 0..name_entries {
            let ne = &set[2 + k];
            if ne[0] != 0xC1 {
                return Err(Error::BadDirEntry);
            }
            for j in 0..15 {
                let idx = k * 15 + j;
                if idx < name_len {
                    ent.name[idx] = le16(ne, 2 + 2 * j);
                }
            }
        }
        if self.check_names && self.name_hash(ent.name_utf16()) != le16(st, 4) {
            return Err(Error::BadDirEntry);
        }
        if ent.valid_data_length > ent.data_length {
            return Err(Error::BadDirEntry);
        }
        Ok(Raw::File(ent))
    }

    /// Prossima voce (file o cartella) della cartella; `None` alla fine.
    pub fn read_dir(&mut self, d: &mut Dir) -> Result<Option<DirEntry>> {
        loop {
            match self.next_raw(d)? {
                Raw::End => return Ok(None),
                Raw::File(e) => return Ok(Some(e)),
                _ => {}
            }
        }
    }

    fn up(&self, c: u16) -> u16 {
        if (c as usize) < UPCASE_LEN {
            self.upcase[c as usize]
        } else {
            c
        }
    }

    fn name_hash(&self, name: &[u16]) -> u16 {
        let mut h: u16 = 0;
        for &c in name {
            let u = self.up(c);
            for b in u.to_le_bytes() {
                h = h.rotate_right(1).wrapping_add(b as u16);
            }
        }
        h
    }

    fn load_upcase(&mut self, checksum: u32, first: u32, len: u64) -> Result<()> {
        if len == 0 || len > 128 * 1024 || len % 2 != 0 {
            return Err(Error::UpcaseChecksum);
        }
        let mut s = Stream::new(first, len, len, false);
        let mut sum: u32 = 0;
        let mut idx: usize = 0;
        let mut pending_run = false;
        let mut chunk = [0u8; 512];
        let mut table = self.upcase;
        loop {
            let n = self.stream_read(&mut s, &mut chunk)?;
            if n == 0 {
                break;
            }
            for &x in &chunk[..n] {
                sum = sum.rotate_right(1).wrapping_add(x as u32);
            }
            for w in chunk[..n].chunks_exact(2) {
                let v = u16::from_le_bytes([w[0], w[1]]);
                if pending_run {
                    // Tabella compressa: 0xFFFF, n = n caratteri che restano uguali.
                    idx += v as usize;
                    pending_run = false;
                } else if v == 0xFFFF {
                    pending_run = true;
                } else {
                    if idx < UPCASE_LEN {
                        table[idx] = v;
                    }
                    idx += 1;
                }
            }
        }
        if sum != checksum {
            return Err(Error::UpcaseChecksum);
        }
        self.upcase = table;
        self.info.upcase_loaded = true;
        Ok(())
    }

    fn name_eq(&self, a: &[u16], b: &[u16]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(&x, &y)| self.up(x) == self.up(y))
    }

    /// Cerca un percorso (`/` o `\` come separatori, maiuscole indifferenti).
    /// Il percorso vuoto o `/` è la radice, restituita come voce cartella.
    pub fn lookup(&mut self, path: &str) -> Result<DirEntry> {
        let mut cur = DirEntry {
            name: [0; MAX_NAME],
            name_len: 0,
            attributes: ATTR_DIRECTORY,
            first_cluster: self.root_cluster,
            data_length: 0,
            valid_data_length: 0,
            no_fat_chain: false,
        };
        let mut at_root = true;
        for comp in path.split(['/', '\\']).filter(|c| !c.is_empty()) {
            let mut want = [0u16; MAX_NAME];
            let mut wn = 0;
            for u in comp.encode_utf16() {
                if wn == MAX_NAME {
                    return Err(Error::NameTooLong);
                }
                want[wn] = u;
                wn += 1;
            }
            if !cur.is_dir() {
                return Err(Error::NotADirectory);
            }
            let mut d = if at_root { self.root()? } else { self.open_dir(&cur)? };
            let mut found = None;
            while let Some(e) = self.read_dir(&mut d)? {
                if self.name_eq(e.name_utf16(), &want[..wn]) {
                    found = Some(e);
                    break;
                }
            }
            cur = found.ok_or(Error::NotFound)?;
            at_root = false;
        }
        Ok(cur)
    }

    /// Apre un file dal percorso.
    pub fn open_path(&mut self, path: &str) -> Result<File> {
        let e = self.lookup(path)?;
        self.open_file(&e)
    }

    /// Apre una cartella dal percorso.
    pub fn open_dir_path(&mut self, path: &str) -> Result<Dir> {
        let e = self.lookup(path)?;
        if e.name_len == 0 {
            return self.root();
        }
        self.open_dir(&e)
    }
}
