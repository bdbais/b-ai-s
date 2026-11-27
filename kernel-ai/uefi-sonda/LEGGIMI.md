# Sonda della chiavetta (B-AI-S, Fase 4)

`dist/BOOTX64.EFI` è un'applicazione UEFI piccola (84 KB). All'avvio guarda tutti i dischi interi che il firmware vede, chiavette USB comprese, e in sola lettura:

1. trova il volume exFAT (GPT, MBR o disco senza tabella);
2. elenca la radice;
3. legge per intero `/modelli/inkling-2-mini.gguf` (se manca, il `.gguf` più grande in `/modelli`) e ne calcola lo sha256;
4. se accanto c'è `inkling-2-mini.gguf.sha256`, lo confronta e scrive **VERIFICATO** o **DIVERSO**.

Il file system lo legge `bais-storage`, la stessa libreria Rust `no_std` che entrerà in kernel-ai. Per ora i blocchi arrivano dal driver USB del firmware (protocollo BlockIo). I driver propri (xHCI + mass storage, AHCI/NVMe) sono il passo successivo e useranno la stessa interfaccia `BlockDevice`.

## Prova su baisryzen7

Servono una chiavetta con due partizioni, oppure due chiavette: una FAT32 per l'avvio e una exFAT per il modello.

1. **Avvio (FAT32).** Copia `dist/BOOTX64.EFI` in `EFI\BOOT\BOOTX64.EFI` su una chiavetta FAT32.
2. **Modello (exFAT).** Su una chiavetta o partizione exFAT crea la cartella `modelli` e copiaci il GGUF di Inkling 2 Mini (17,63 GB).
3. **Hash atteso.** Su Windows:
   `powershell -ExecutionPolicy Bypass -File tools\scrivi-sha256.ps1 E:\modelli\inkling-2-mini.gguf`
   (sostituisci `E:` con la lettera della chiavetta). Scrive `inkling-2-mini.gguf.sha256` accanto al modello.
4. **Secure Boot.** L'applicazione non è firmata: per la prova va disattivato nel BIOS, da riattivare dopo.
5. **Avvio.** Riavvia, apri il menu di avvio del BIOS e scegli la chiavetta FAT32 in modalità UEFI.
6. A video compaiono volume, radice, avanzamento ogni GiB, velocità, sha256 ed esito. Su un PC vero il risultato resta a video 60 secondi, poi il PC si spegne.

La sonda non scrive su nessun disco. Legge anche i dischi interni: se lì trova un volume exFAT, ne elenca la radice e cerca lo stesso file.

## Costruire e provare in QEMU (Linux)

    ./build.sh                       # gnu-efi + Rust stabile (target x86_64-unknown-linux-gnu)
    sudo tools/prova-qemu.sh 256     # chiavetta GPT finta con un «modello» da 256 MiB, OVMF, USB xHCI
