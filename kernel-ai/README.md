# kernel-ai / gestore modelli (Fase 3)

`model/` — crate `bais-model`, `no_std` + `alloc`: registro, caricamento GGUF senza copie,
scheduler, campionamento, comandi di terminale (`modelli`, `carica`, `scarica`, `predefinito`, `genera`)
e motore di inferenza su CPU. Architetture: `llama` (denso e MoE) e `inkling2` / `inkling2_loop`
(Inkling 2 Mini). Formati: F32, F16, Q8_0, Q4_0, IQ4_NL.

`model-cli/` — lo stesso motore su host, per il collaudo.

`tests/` — generatori di modelli minuscoli a pesi casuali (stessa architettura degli originali),
riferimento llama.cpp (`ref.cpp`) e confronto token per token (`cmp.sh`).

## Collaudo (2026-11-22, llama.cpp master 94f3af3)
- llama F32/Q8_0, denso e MoE: 4 modelli × 5 prompt, 32 token greedy identici.
- Inkling 2 e Inkling 2 loop (3 passate), F32: identici; logit entro 3e-5.
- Q8_0 Inkling non-loop: identico (logit entro 1e-6) tranne un prompt; Q8_0 loop: identico per i primi
  15–24 token, poi diverge per un arrotondamento int8 a pareggio (x·id = -34.4999962): rumore
  in virgola mobile, non logica.
- Tokenizer BPE con pre-split GPT-4o: identico su tutti i prompt di prova.

## Nuclei AVX2 (2026-11-24)
- `model/src/simd.rs`: rilevamento con CPUID + XGETBV (niente std), scelta a tempo di esecuzione,
  ripiego scalare. `simd::set_simd(false)` o `BAIS_NOSIMD=1` nel CLI forzano lo scalare.
- Q8_0, Q4_0, IQ4_NL: somme intere 8 blocchi alla volta; F32: mul + add senza FMA.
  Risultati identici bit per bit allo scalare (test `simd_uguale_allo_scalare`) e token identici
  su 8 modelli × 5 prompt × 32 token.
- `bais-model-cli --bench` (4096×4096, 2 vCPU condivise): Q8_0 ×1,6, Q4_0 ×1,5–2,4, IQ4_NL ×3,4, F32 ×6,5.
- Nel kernel serve abilitare AVX: CR4.OSXSAVE e XCR0 (bit 1 e 2) prima di usare il modello,
  altrimenti il rilevamento sceglie da solo il percorso scalare.


## Kernel che usano il motore
- `kernel-pc/` — v0.3 per PC x86_64 (GRUB multiboot2); il modello di prova arriva come modulo GRUB.
- `kernel-rpi/` — v0.3 per Raspberry Pi 5 (ARM64 bare metal). Si compila sul PC con `build.ps1`
  (serve `rustup target add aarch64-unknown-none`).

## Collaudo nel kernel (2026-11-25, kernel-pc v0.3, QEMU 8.2, llama.cpp master ea72294)
- `modelli` elenca i modelli registrati dai moduli GRUB; `carica 1` carica e mostra la memoria usata.
- `token <testo>` stampa gli id greedy nel formato di `tests/ref`: sui 5 prompt di `cmp.sh`,
  32 token identici a llama.cpp sul modello di prova Q8_0 (prompt compreso).
- Scheduler: `Task::Classify` sceglie il classificatore degli intenti (`bais.role = "intent"`),
  caricato all'avvio. `bais-intent` (`tests/gen_intent.py`, llama a caratteri addestrato da noi)
  trasforma le frasi sul tema in etichette (`#macos #scuro …`) che il kernel applica al tema;
  le parole chiave della v0.2 restano solo come ripiego. `capisci <frase>` mostra le etichette.

## Memoria di massa (Fase 4, 2026-11-27)
- `bais-storage/` — `no_std`, senza `alloc` né dipendenze: GPT/MBR, exFAT in sola lettura (file oltre
  i 4 GB) e SHA-256 a flusso. Un driver implementa `BlockDevice` e il file system resta lo stesso.
  Collaudo su immagini di exfatprogs/exfat-fuse: 40 verifiche su 40, file da 17,63 GB compreso.
- `uefi-sonda/` — `BOOTX64.EFI`: avvio da chiavetta, legge `/modelli/inkling-2-mini.gguf` e ne verifica
  lo sha256. In QEMU + OVMF, chiavetta USB xHCI: 17,63 GB letti e VERIFICATO. Prova su PC vero: `uefi-sonda/LEGGIMI.md`.

## Da fare
- Build per `x86_64-unknown-none` (il crate è già `no_std`; serve il target installato).
- Prestazioni: scala f16 con F16C, multi-core (Fase 4).
