# Roadmap

La strada è a piccoli passi, ognuno verificabile in una macchina virtuale prima che su hardware reale.
*Small steps, each verifiable in a VM before real hardware.*

## ✅ Tappa 0 — Kernel avviabile (prototipo v0.1)
- Rust bare-metal x86_64, Multiboot2 via GRUB, long mode.
- Console VGA testo + seriale, tastiera PS/2, heap, IDT per le eccezioni CPU.
- Shell `bais>` con router di intenti **a regole**: app, servizio, agente, tema, chat.
- Brain in RAM (`ricorda:` / `cerca`).

## ✅ Tappa 1 — Desktop grafico generativo (prototipo v0.2)
- Framebuffer 1024×768×32, finestre, dock/barra, mouse PS/2, font con antialiasing.
- Interfaccia modificabile da frase, anche con richieste combinate; `annulla`.
- Seme delle abitudini: sequenze ripetute → il sistema chiede se può eseguirle da solo.
- Comando `hardware` con proposte di ottimizzazione.

## 🔜 Tappa 2 — Il primo modello
- Un classificatore di intenti minuscolo (KB, non GB), addestrato da noi, che gira nel kernel al posto delle parole chiave.
- Dataset aperto di frasi → intenti, in più lingue. **Contributi benvenuti.**

## ⏳ Tappa 3 — Memoria che resta
- Driver disco (AHCI/NVMe), file system semplice, persistenza di brain, temi e abitudini.
- Multi-core (SMP), interrupt hardware, scheduler.

## ⏳ Tappa 4 — Un modello linguistico su CPU
- Inferenza di un piccolo LLM quantizzato su CPU (AVX2/FMA).
- Scelta architetturale aperta: runtime nel kernel o kernel minimo + userspace. Vedi [DOMANDE-APERTE](DOMANDE-APERTE.md).

## ⏳ Tappa 5 — Dalla chiavetta al PC reale
- Avvio UEFI, framebuffer GOP, tastiera/mouse USB (xHCI), immagine per chiavetta.

## ⏳ Tappa 6 — Voce
- Driver audio, speech-to-text e text-to-speech locali e leggeri.

## ⏳ Tappa 7 — Il brain che impara
- Brain locale persistente e multimodale (testo, immagini, audio, video), indici vettoriali, RAG.
- Auto-apprendimento: il sistema affina i propri modelli locali con il feedback, in modo trasparente e reversibile.
- Agenti come organi del sistema che comunicano tra loro.
