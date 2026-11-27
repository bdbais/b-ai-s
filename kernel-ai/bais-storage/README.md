# bais-storage

Pezzo della **Fase 4** di kernel-ai (B-AI-S): leggere dalla chiavetta i file di modello da molti GB.

- `block`: il contratto con il driver (`BlockDevice`: dimensione del blocco, numero di blocchi, lettura di blocchi contigui) e `Partition`, una vista che non esce dai confini della partizione.
- `gpt`: GPT con verifica CRC32 di intestazione e voci, ripiego sulla copia di riserva in fondo al disco; MBR con le 4 primarie.
- `exfat`: exFAT in sola lettura. Controlla checksum di avvio, catene dei cluster (cicli e catene troncate scoperti all'apertura), checksum e hash dei nomi. Confronta i nomi senza distinguere le maiuscole con la tabella del volume (U+0000..U+052F) e legge i file oltre i 4 GB.
- `sha256`: SHA-256 a flusso, per verificare i pesi mentre si leggono.

`no_std`, niente `alloc`, niente `unsafe`, nessuna dipendenza. Il tipo `Volume` occupa circa 11 KB: nel kernel va messo nell'heap o in una statica, non sullo stack di un'interruzione.

## Uso

```rust
let (start, len) = bais_storage::find_exfat(&mut disco)?;          // disco: impl BlockDevice
let mut vol = exfat::Volume::open(Partition::new(&mut disco, start, len))?;
let mut f = vol.open_path("/modelli/inkling-2-mini.gguf")?;
let mut h = Sha256::new();
loop {
    let n = vol.read(&mut f, &mut buf)?;   // buf grande (es. 8 MB): letture di molti blocchi contigui
    if n == 0 { break }
    h.update(&buf[..n]);
}
```

Per mappare i pesi senza copiarli, `File::seek` + `Volume::read` leggono qualsiasi intervallo. La paginazione su richiesta dal disco verrà sopra questo, nella Fase 4.

## Collaudo

    cargo test                                  # vettori CRC32 e SHA-256, limiti delle partizioni
    sudo python3 tests/real_images.py --big 16.42

`tests/real_images.py` crea immagini con un'implementazione indipendente (mkfs.exfat di exfatprogs, file scritti con exfat-fuse) e confronta, file per file, albero, dimensioni, sha256 e letture in posizioni a caso:

- A: volume senza tabella, settori da 512, 308 file, nomi accentati, emoji, nome di 204 caratteri, cartella su più cluster;
- B: chiavetta GPT come quella vera (ESP FAT32 + exFAT), cluster da 4 KB, file frammentati;
- B2: GPT principale rovinata (si usa la copia di riserva) e poi entrambe (errore chiaro);
- C: MBR tipo 0x07;
- D: disco con settori da 4096 byte, letto con blocchi da 4096 e da 512;
- E: corruzioni: settore di avvio, nome di un file, ciclo nella FAT;
- F: un file da 17,63 GB, la dimensione di Inkling 2 Mini IQ4_NL.

Ultimo risultato (2026-11-27): 40 verifiche su 40. Sul file da 17,63 GB lo sha256 è identico a quello di Python: 236 MB/s sul contenitore, limitati dallo SHA-256 scalare.

## Strumento `bais-disk`

    cargo run --release --example bais-disk -- tree chiavetta.img
    cargo run --release --example bais-disk -- sha  chiavetta.img /modelli/inkling-2-mini.gguf

Usa la stessa libreria su un file immagine.
