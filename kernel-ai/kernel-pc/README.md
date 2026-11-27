# kernel-ai v0.3-pc (B-AI-S)

La v0.3 per PC x86_64: lo stesso sistema della v0.3-rpi (desktop che si modifica da frase,
diario, abitudini, brain in RAM, splash di nascita calcolata dal corpo, gestore modelli
`bais-model` della Fase 3) con il livello di piattaforma riscritto per il PC.

## Cosa cambia rispetto alla v0.3-rpi

- Avvio: GRUB multiboot2 (BIOS) → modo lungo, mappa identità dei primi 4 GiB a pagine da 2 MiB.
- Schermo: framebuffer VBE 1024×768×32 chiesto a GRUB.
- Input: tastiera PS/2 con **layout italiano** (accentate, AltGr per `[ ] { } @ # €`),
  oppure seriale COM1. `tastiera us` / `tastiera it` per cambiare layout.
- Corpo: CPUID (nome, thread, AVX2/FMA), RAM dalla mappa di GRUB, SMBIOS (produttore, modello, UUID),
  TSC tarato col PIT, orologio dal CMOS. L'impronta di nascita usa UUID, CPU, RAM e schermo.
- Modello: modulo multiboot2 (`module2` in `boot/grub.cfg`). L'ISO contiene il modello di prova
  Q8_0 generato da `../tests/gen_llama.py` (pesi casuali: il testo generato non ha senso).
- Nuovi comandi: `tastiera it|us`, `riavvia`, `token <testo>` (id greedy, per il confronto con llama.cpp),
  `capisci <frase>` (etichette del classificatore).
- Classificatore degli intenti `bais-intent` (secondo modulo GRUB, generato da `../tests/gen_intent.py`):
  caricato all'avvio, lo scheduler lo usa per le frasi sul tema; le parole chiave restano come ripiego.

## Collaudo automatico in QEMU

    python3 tools/qemu_drive.py dist/bais-kernel-v0.3-pc.iso /tmp/seriale.log "modelli" "carica 1" "token the cat sat"

Avvia QEMU senza finestra, scrive le frasi sulla seriale COM1 e salva tutta l'uscita
(`CPU`, `MEM`, `QUIET`, `MAXS`, `SCREEN=<file.ppm>` per una schermata finale).

## Compilare

    tools/mkiso.sh        # → dist/bais-kernel-v0.3-pc.iso

Target provvisorio `x86_64-unknown-linux-gnu` freestanding (opzioni in `cargo-config.toml`, passato a cargo da `tools/mkiso.sh`), come v0.1/v0.2.

## Provare

- Bochs (Windows): `Download\bais-vm\avvia-bais-v03.bat`.
- QEMU: `qemu-system-x86_64 -m 512 -cdrom dist/bais-kernel-v0.3-pc.iso -serial stdio`.

## Verifiche (2026-11-25)

QEMU 8.2 e Bochs 2.7 (BIOS di Bochs 3.1): boot fino al desktop, splash, stili da frase,
`hardware`, tastiera italiana, `carica 1` + `genera ciao` con testo identico tra QEMU e Bochs.

Nuclei AVX2 (2026-11-25): `boot.s` accende CR4.OSXSAVE e XCR0 (x87, SSE, AVX) se la CPU ha
XSAVE e AVX. In QEMU `-cpu max` `corpo` riporta i nuclei AVX2 attivi, con `-cpu qemu64` il
percorso scalare; `genera` produce gli stessi token nei due casi. In QEMU l'AVX è emulato e
quindi più lento: la misura vera si fa sul PC (`bais-model-cli --bench` su host: ×1,2–4,1).

## Limiti

- Un solo core, nessuna persistenza, niente mouse, niente sensori di temperatura.
- Solo BIOS: per il PC reale serve l'avvio UEFI (Fase 4).
- In Bochs il disegno è lento: mentre esegue una frase la tastiera non viene letta.
