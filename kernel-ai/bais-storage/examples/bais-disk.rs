//! Strumento di prova su PC: usa la stessa libreria del kernel su un file
//! immagine (o su un disco, se il sistema lo permette).
//!
//!   bais-disk parts <img> [bs]                 partizioni
//!   bais-disk info  <img> [bs]                 volume exFAT trovato
//!   bais-disk tree  <img> [bs]                 elenco ricorsivo: tipo, dimensione, percorso
//!   bais-disk sha   <img> <percorso> [bs]      sha256 del file, tempo e velocità
//!   bais-disk range <img> <percorso> <da> <n> [bs]   n byte dalla posizione «da», su stdout
//!
//! `bs` = dimensione del blocco del dispositivo simulato (512 di default).

use bais_storage::exfat::{Dir, Volume};
use bais_storage::gpt::{self, Scheme};
use bais_storage::sha256::{to_hex, Sha256};
use bais_storage::{find_exfat, BlockDevice, Error, Partition, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::time::Instant;

struct ImgDev {
    f: File,
    bs: u32,
    blocks: u64,
    pub requests: u64,
}

impl ImgDev {
    fn open(path: &str, bs: u32) -> ImgDev {
        let f = File::open(path).expect("immagine");
        let len = f.metadata().unwrap().len();
        ImgDev { f, bs, blocks: len / bs as u64, requests: 0 }
    }
}

impl BlockDevice for ImgDev {
    fn block_size(&self) -> u32 {
        self.bs
    }
    fn num_blocks(&self) -> u64 {
        self.blocks
    }
    fn read_blocks(&mut self, lba: u64, buf: &mut [u8]) -> Result<()> {
        self.requests += 1;
        if buf.len() % self.bs as usize != 0 || lba + (buf.len() / self.bs as usize) as u64 > self.blocks {
            return Err(Error::OutOfRange);
        }
        self.f.seek(SeekFrom::Start(lba * self.bs as u64)).map_err(|_| Error::Io)?;
        self.f.read_exact(buf).map_err(|_| Error::Io)
    }
}

fn die(e: Error) -> ! {
    eprintln!("errore: {} ({})", e.as_str(), e.code());
    std::process::exit(2)
}

fn volume(img: &str, bs: u32) -> Volume<Partition<ImgDev>> {
    let mut dev = ImgDev::open(img, bs);
    let (start, len) = find_exfat(&mut dev).unwrap_or_else(|e| die(e));
    Volume::open(Partition::new(dev, start, len)).unwrap_or_else(|e| die(e))
}

fn tree<D: BlockDevice>(v: &mut Volume<D>, d: &mut Dir, prefix: &str) {
    let mut ents = Vec::new();
    while let Some(e) = v.read_dir(d).unwrap_or_else(|e| die(e)) {
        ents.push(e);
    }
    for e in ents {
        let mut b = [0u8; 765];
        let name = e.name_utf8(&mut b).unwrap().to_string();
        let path = format!("{prefix}/{name}");
        if e.is_dir() {
            println!("d\t0\t{path}");
            let mut sd = v.open_dir(&e).unwrap_or_else(|e| die(e));
            tree(v, &mut sd, &path);
        } else {
            println!("f\t{}\t{path}\t{}\t{}", e.data_length, if e.no_fat_chain { "contigua" } else { "catena" }, e.first_cluster);
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let cmd = a.get(1).map(String::as_str).unwrap_or("");
    let bs_at = |i: usize| a.get(i).map(|s| s.parse().unwrap()).unwrap_or(512u32);
    match cmd {
        "parts" => {
            let mut dev = ImgDev::open(&a[2], bs_at(3));
            match gpt::for_each_partition(&mut dev, |_, p| {
                let name: String = char::decode_utf16(p.name.iter().copied().take_while(|&c| c != 0))
                    .map(|c| c.unwrap_or('?'))
                    .collect();
                let kind = if p.type_guid == gpt::GUID_BASIC_DATA {
                    "basic-data"
                } else if p.type_guid == gpt::GUID_EFI_SYSTEM {
                    "efi-system"
                } else {
                    "altro"
                };
                println!("{:?}\t{}\t{}\t{}\t{}\t0x{:02x}\t{}", p.scheme, p.index, p.first_lba, p.last_lba, kind, p.mbr_type, name);
                true
            }) {
                Ok(Scheme::Gpt) => println!("schema GPT"),
                Ok(Scheme::Mbr) => println!("schema MBR"),
                Err(e) => println!("nessuna tabella: {}", e.as_str()),
            }
        }
        "info" => {
            let v = volume(&a[2], bs_at(3));
            let i = v.info();
            let label: String = char::decode_utf16(i.label[..i.label_len as usize].iter().copied())
                .map(|c| c.unwrap_or('?'))
                .collect();
            println!(
                "settore {} cluster {} cluster_tot {} settori {} seriale {:08x} rev {}.{} etichetta «{}» maiuscole {}",
                i.bytes_per_sector,
                i.cluster_bytes,
                i.cluster_count,
                i.volume_sectors,
                i.serial,
                i.revision >> 8,
                i.revision & 0xff,
                label,
                i.upcase_loaded
            );
        }
        "tree" => {
            let mut v = volume(&a[2], bs_at(3));
            let mut r = v.root().unwrap_or_else(|e| die(e));
            tree(&mut v, &mut r, "");
        }
        "sha" => {
            let mut v = volume(&a[2], bs_at(4));
            let mut f = v.open_path(&a[3]).unwrap_or_else(|e| die(e));
            let chunk: usize = std::env::var("CHUNK").ok().and_then(|s| s.parse().ok()).unwrap_or(8 << 20);
            let mut buf = vec![0u8; chunk];
            let mut h = Sha256::new();
            let t = Instant::now();
            let mut total = 0u64;
            loop {
                let n = v.read(&mut f, &mut buf).unwrap_or_else(|e| die(e));
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
                total += n as u64;
            }
            let dt = t.elapsed().as_secs_f64();
            let mut hex = [0u8; 64];
            to_hex(&h.finish(), &mut hex);
            let reqs = v.into_inner().into_inner().requests;
            println!(
                "{}  {}\t{} byte\t{:.2} s\t{:.0} MB/s\t{} richieste",
                std::str::from_utf8(&hex).unwrap(),
                a[3],
                total,
                dt,
                total as f64 / dt.max(1e-9) / 1e6,
                reqs
            );
        }
        "range" => {
            let mut v = volume(&a[2], bs_at(6));
            let mut f = v.open_path(&a[3]).unwrap_or_else(|e| die(e));
            f.seek(a[4].parse().unwrap());
            let mut buf = vec![0u8; a[5].parse().unwrap()];
            let n = v.read(&mut f, &mut buf).unwrap_or_else(|e| die(e));
            std::io::stdout().write_all(&buf[..n]).unwrap();
        }
        _ => {
            eprintln!("uso: bais-disk parts|info|tree|sha|range ...");
            std::process::exit(1)
        }
    }
}
