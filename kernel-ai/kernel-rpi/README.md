# kernel-ai per Raspberry Pi 5 — prototipo v0.3-rpi

Porting ARM64 bare metal di kernel-ai (B-AI-S). Stessa idea della v0.2 x86: il kernel disegna un
desktop che si modifica da frase, tiene diario e abitudini, ha un brain in RAM e risponde al comando
`hardware`. In più, rispetto alla v0.2, qui il gestore modelli `bais-model` (Fase 3) è già agganciato:
`modelli`, `carica`, `scarica`, `predefinito`, `genera`.

Una sola immagine per tutte le schede: il core viene riconosciuto da MIDR_EL1 e da lì si ricavano
gli indirizzi delle periferiche.

| Scheda | SoC / core | Immagine | Stato |
|---|---|---|---|
| Raspberry Pi 5 / 500 / CM5 | BCM2712, Cortex-A76 | `kernel_2712.img` | obiettivo, **non ancora provato su scheda vera** |
| Raspberry Pi 4 / 400 | BCM2711, Cortex-A72 | `kernel8.img` | non provato |
| Raspberry Pi 3 / Zero 2 W | BCM2837, Cortex-A53 | `kernel8.img` | collaudato in QEMU `raspi3b` |

QEMU 8.2 emula al massimo il Pi 3 (il Pi 4 arriva con QEMU 9, il Pi 5 non è emulato). Il codice del
Pi 5 differisce solo negli indirizzi (mailbox `0x10_7C01_3880`, UART di debug `0x10_7D00_1000`,
gli stessi della libreria Circle) e nel modo in cui il firmware fornisce lo schermo.

## Cosa c'è

- Avvio a 0x80000 da EL2 (o EL3) a EL1, core secondari parcheggiati, FP/NEON attivi.
- MMU con mappa identità: RAM con cache, periferiche Device, framebuffer non-cacheable.
- UART PL011: si scrive al sistema da un terminale seriale (115200 8N1).
- Mailbox del firmware: revisione della scheda, memoria, seriale, temperatura del SoC, clock ARM, framebuffer.
- Device tree: nome della scheda e initramfs (dove il firmware mette il file del modello).
- Desktop 1280×720 (o la risoluzione che il firmware impone sul Pi 5): finestre Terminale, Brain e
  Sistema, dock o barra di Windows, barra dei menu, campo di richiesta, font Inter con antialiasing.
- Stili e modifiche da frase come nella v0.2 (`macos scuro con la barra di windows e accento viola`), `annulla`.
- Diario e abitudini: se una coppia di frasi si ripete due volte chiede una volta se può eseguirla da solo.
- Brain in RAM: `ricorda: …`, `cerca …`.
- Nascita: all'avvio un'impronta generativa calcolata dal corpo (seriale, core, memoria, revisione).
  Su un'altra scheda l'impronta cambia.
- `hardware`: scheda, core, clock, memoria, temperatura, SIMD (NEON, dotprod, LSE) e proposte per
  sfruttare il corpo (altri 3 core, SDOT per i prodotti Q8, limiti di memoria per Inkling).
- Schermata d'errore su eccezione della CPU o panic, anche sulla seriale.

## Compilare (Windows o Linux)

```
rustup target add aarch64-unknown-none
cd kernel-rpi
.\build.ps1        # Windows (PowerShell)
./build.sh         # Linux
```

In `dist/` trovi `kernel_2712.img`, `kernel8.img` (sono lo stesso file) e `config.txt`. A mano: `cargo build --release --target aarch64-unknown-none`.
L'ELF diventa immagine grezza con `tools/mkimg.py` (solo Python standard, niente objcopy).

## Preparare la scheda SD

1. Con Raspberry Pi Imager scrivi **Raspberry Pi OS Lite (64-bit)** sulla SD: serve solo per avere
   firmware, device tree (`bcm2712-rpi-5-b.dtb`) e `overlays/` (per i Pi 5 con SoC D0 serve
   `overlays/bcm2712d0.dtbo`) aggiornati.
2. Nella partizione `bootfs` rinomina `config.txt` in `config-raspios.txt` e copia i tre file di `dist/`.
3. Facoltativo, modello: copia un `.gguf` come `bais-model.gguf` e togli il `#` dalla riga
   `initramfs` in `config.txt`. Oggi solo modelli piccoli (la RAM usabile è il primo GB).

Per tornare a Raspberry Pi OS basta rimettere il `config.txt` originale.

## Parlare con il sistema

Tastiera e mouse USB sul Pi 5 passano dal chip RP1 sul PCIe: il driver non c'è ancora. Per ora si
scrive dalla **UART di debug** (connettore a 3 pin tra le due micro-HDMI) con una Raspberry Pi
Debug Probe o un adattatore USB-seriale a 3,3 V: PuTTY o Tera Term, porta COM dell'adattatore,
115200 8N1. Su Pi 3/4 la UART è sui GPIO14/15 (pin 8 e 10). Il desktop esce sull'HDMI.

Prova: `aiuto`, `hardware`, `temperatura`, `macos scuro con la barra di windows e accento viola`,
`annulla`, `ricorda: …`, `cerca …`, `modelli`, `carica 1`, `genera ciao`.

## Collaudo fatto (QEMU 8.2, macchina raspi3b)

- Avvio fino al desktop; splash di nascita; framebuffer 1280×720.
- Tutti gli stili, frasi combinate, `annulla`; abitudine proposta, accettata ed eseguita da sola.
- `hardware`, `temperatura`, brain (`ricorda`, `cerca`).
- Modello dall'initramfs (device tree) e, senza device tree, da indirizzo fisso 0x08000000.
- `genera` con i modelli di prova F32 e Q8_0 (`tests/gen_llama.py`): testo **identico byte per byte**
  a quello del motore compilato per x86 sulle stesse condizioni. ~80 token/s su un core emulato.
- `tools/qemu-test.sh [modello.gguf]` ripete il collaudo.

## Limiti

- Mai avviato su un Pi 5 vero. Punti da guardare per primi: risoluzione HDMI imposta dal firmware
  (gestita: il kernel usa quella che riceve), baud della UART di debug (se il firmware l'ha già
  accesa con `enable_uart=1` il kernel non tocca i baud).
- Un solo core; motore di inferenza scalare (né NEON né SDOT ancora).
- Heap nel primo GB di RAM; il resto è mappato ma non usato.
- Niente USB, rete, disco, audio sul Pi 5 (tutto dietro l'RP1). Diario, brain e tema si perdono allo spegnimento.
- Interprete a parole chiave, come la v0.2.
