# Provare B-AI-S — macchina virtuale e chiavetta USB

🇮🇹 Italiano · [🇬🇧 English summary](#-english-summary)

> ⚠️ **Prototipo sperimentale.** B-AI-S oggi è un kernel dimostrativo. Non salva niente su disco, non tocca i dischi del computer e si spegne senza lasciare tracce, ma **non è un sistema da usare per lavorare**. Prima prova in macchina virtuale.

## Cosa serve sapere prima

| | Stato attuale (prototipo v0.2) |
|---|---|
| Architettura | PC x86_64 (64 bit) |
| Avvio | **BIOS / Legacy** tramite GRUB (Multiboot2). **UEFI non ancora supportato** |
| Memoria | almeno 256 MB |
| Schermo | 1024×768 a 32 bit; se non è disponibile si usa la modalità testo |
| Tastiera e mouse | **PS/2**. Le periferiche USB funzionano solo se il BIOS le emula come PS/2 ("USB Legacy Support") |
| Layout tastiera | US |
| Disco, rete, audio, USB | non ancora supportati |

## 1. Scaricare l'immagine

Scarica `bais-kernel-v0.2.iso` dalla pagina [**Releases**](https://github.com/bdbais/b-ai-s/releases) del repository.

> I sorgenti del kernel e le istruzioni per compilarlo da soli verranno pubblicati qui a breve.

## 2. Provare in macchina virtuale (consigliato)

### QEMU — Linux, macOS, Windows

QEMU è l'emulatore usato per sviluppare e verificare il prototipo (provato con QEMU 8.2).

**Installazione**
- Debian/Ubuntu: `sudo apt install qemu-system-x86`
- Fedora: `sudo dnf install qemu-system-x86`
- macOS (Homebrew): `brew install qemu`
- Windows: installer da [qemu.org/download](https://www.qemu.org/download/#windows)

**Avvio**

```bash
qemu-system-x86_64 -cdrom bais-kernel-v0.2.iso -m 256M -cpu max -serial stdio
```

- `-cpu max` espone al kernel tutte le istruzioni che l'emulatore conosce (AVX2, FMA…), utili al comando `hardware`.
- `-serial stdio` mostra nel terminale anche l'output della porta seriale: utile per i log e per segnalare problemi.
- Per catturare il mouse clicca nella finestra di QEMU; per liberarlo premi `Ctrl+Alt+G`.

### Bochs — Windows (versione portatile)

1. Scarica Bochs 2.7 o successivo da [bochs.sourceforge.io](https://bochs.sourceforge.io/).
2. Metti in una cartella `bochs.exe`, la ISO e un file `bochsrc.txt` come questo:

   ```
   megs: 256
   romimage: file=$BXSHARE/BIOS-bochs-latest
   vgaromimage: file=$BXSHARE/VGABIOS-lgpl-latest
   ata0-master: type=cdrom, path="bais-kernel-v0.2.iso", status=inserted
   boot: cdrom
   mouse: enabled=0, type=ps2
   clock: sync=realtime
   ```

3. Avvia con `bochs.exe -q -f bochsrc.txt` (puoi salvarlo in un file `avvia-bais.bat`).

Sotto Bochs l'avvio fino al desktop è verificato; **tastiera e mouse sotto Bochs non sono ancora stati provati**: se li provi, raccontaci com'è andata.

### VirtualBox / VMware

Non ancora provati. In teoria funzionano creando una VM "Other / Unknown (64-bit)" con **firmware BIOS (EFI disattivato)**, 256 MB di RAM e la ISO come CD di avvio. Se li provi, apri una issue con il risultato.

## 3. Provare da chiavetta USB su un PC reale

> ⚠️ **Scrivere la ISO sulla chiavetta cancella tutto il suo contenuto.** Controlla due volte di aver scelto la chiavetta giusta.
> Su PC reale il prototipo **non è ancora stato provato**: è un esperimento, ed è proprio uno dei punti dove cerchiamo aiuto.

### Requisiti del PC
- Firmware con avvio **Legacy / CSM** (molti PC moderni sono solo UEFI: in quel caso oggi non parte).
- Secure Boot disattivato.
- Tastiera PS/2, oppure "USB Legacy Support" attivo nel BIOS.

### Creare la chiavetta

**Windows — Rufus**
1. Scarica [Rufus](https://rufus.ie/).
2. Seleziona la chiavetta e la ISO.
3. Schema di partizione **MBR**, sistema di destinazione **BIOS (o UEFI-CSM)**.
4. Quando Rufus lo chiede, scegli la scrittura **in modalità immagine DD**.

**Linux / macOS / Windows — balenaEtcher**
1. Scarica [balenaEtcher](https://etcher.balena.io/).
2. *Flash from file* → la ISO → la chiavetta → *Flash*.

**Linux — da terminale**

```bash
lsblk                     # individua la chiavetta, es. /dev/sdX (NON una partizione come /dev/sdX1)
sudo dd if=bais-kernel-v0.2.iso of=/dev/sdX bs=4M status=progress conv=fsync
```

### Avviare
1. Inserisci la chiavetta e accendi il PC.
2. Apri il menu di avvio (di solito `F12`, `F11`, `F8` o `Esc`) e scegli la voce **non UEFI** della chiavetta.
3. Dovresti vedere GRUB e poi il desktop di B-AI-S.

Per uscire usa il comando `spegni`, oppure spegni il PC: nulla viene scritto sui dischi.

## 4. Primi comandi da provare

Scrivi nel campo di richiesta in basso (o nel Terminale):

| Comando | Cosa succede |
|---|---|
| `stile macos`, `stile windows 11`, `mix`, `ripristina` | cambia l'intera interfaccia |
| `macos scuro con la barra di windows e accento viola` | modifiche combinate in una frase |
| `annulla` | torna all'aspetto precedente |
| `ricorda: <testo>` / `cerca <parola>` | scrive e cerca nel brain (in RAM) |
| `abitudini`, `diario` | mostra le abitudini riconosciute e il diario delle frasi |
| `hardware` | descrive la macchina e propone ottimizzazioni |
| `cpu`, `mem`, `ora` | informazioni di sistema |
| `spegni` | spegne il sistema |

Ricorda: oggi l'interprete è a **parole chiave**, non c'è ancora un modello di AI. Capisce solo le formulazioni previste. Proprio per questo cerchiamo aiuto!

## 5. Se qualcosa non va

Apri una [issue](https://github.com/bdbais/b-ai-s/issues/new) indicando:
- emulatore (e versione) o modello del PC;
- come hai creato la chiavetta;
- cosa vedi a schermo (una foto va benissimo);
- se usi QEMU, l'output di `-serial stdio`.

---

## 🇬🇧 English summary

- **Current prototype (v0.2):** x86_64, **BIOS/Legacy boot only** via GRUB Multiboot2 (no UEFI yet), ≥256 MB RAM, 1024×768 framebuffer, PS/2 keyboard and mouse (USB only through BIOS legacy emulation), US layout. No disk, network, audio or USB drivers. Nothing is written to your disks.
- **Get it:** download `bais-kernel-v0.2.iso` from [Releases](https://github.com/bdbais/b-ai-s/releases).
- **VM (recommended):** `qemu-system-x86_64 -cdrom bais-kernel-v0.2.iso -m 256M -cpu max -serial stdio`. Bochs boots to the desktop (input untested); VirtualBox/VMware untested — use BIOS firmware, EFI off.
- **USB stick (untested on real hardware):** write the ISO with Rufus (MBR, BIOS, DD mode), balenaEtcher or `dd`. **This erases the stick.** The PC needs Legacy/CSM boot, Secure Boot off.
- **Try:** `stile macos`, `macos scuro con la barra di windows e accento viola`, `annulla`, `hardware`, `ricorda: …`, `cerca …`, `spegni`.
- **Problems / test reports:** open an issue with your emulator or hardware, how you made the stick, a photo of the screen and the serial log.
