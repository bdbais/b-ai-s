# B-AI-S — Basic Artificial Intelligence System

> **Un sistema operativo che non si usa: si parla.**
> *An operating system you don't operate — you talk to.*

🇮🇹 Italiano · [🇬🇧 English below](#-english)

---

## Il sogno

Oggi i computer ci chiedono di imparare *loro*: menu, cartelle, impostazioni, app che non si parlano. B-AI-S rovescia il rapporto.

B-AI-S è un **sistema operativo generativo**: un kernel che avvia un'intelligenza artificiale e la tratta come il cuore della macchina. Ogni frase diventa ciò che ti aspetti:

- «voglio un'interfaccia stile macOS, ma scura e con la barra di Windows» → il desktop si ridisegna;
- «crea un'app per le note» → l'app nasce, lì, sul momento;
- «fai da server per un servizio contatore» → il sistema diventa quel servizio;
- «crea un agente traduttore» → un nuovo organo del sistema, pronto a rispondere.

Tutto quello che fai, dici e vedi alimenta un **brain locale** che ti conosce sempre meglio — e che **non lascia mai la tua macchina** se non lo decidi tu.

Si installa da una **chiavetta USB**.

👉 La filosofia completa è nel [**Manifesto**](docs/MANIFESTO.md).

## I principi in breve

1. **Locale prima di tutto.** Dati, ricordi e modelli vivono sulla tua macchina. Il cloud è un'opzione, mai un requisito.
2. **L'interfaccia è una conversazione.** Prompt, voce, gesti: l'interfaccia si modifica da sola su richiesta.
3. **Tanti modelli piccoli, non uno gigante.** Classificatori minuscoli, modelli specializzati, agenti-organo: il sistema sceglie quello giusto per ogni compito.
4. **Impara, ma chiede.** Riconosce i lavori ripetitivi e, la prima volta, chiede il permesso di farli da solo.
5. **Si migliora da solo, in modo trasparente.** Addestra e affina i propri modelli locali — e ti mostra cosa ha cambiato e perché.
6. **Conosce il proprio corpo.** Legge l'hardware su cui gira e propone come ottimizzarsi.
7. **Piccolo e comprensibile.** Si parte da un kernel minimo in Rust che chiunque possa leggere.

## A che punto siamo (onestamente)

Siamo all'inizio. C'è un **prototipo di kernel in Rust bare-metal (x86_64)** che si avvia in macchina virtuale:

- desktop grafico disegnato dal kernel (finestre, dock, mouse, font con antialiasing);
- l'interfaccia si modifica da frase: `macos scuro con la barra di windows e accento viola`;
- un primo seme di "abitudini": se ripeti una sequenza, il sistema chiede se può farla da solo;
- comando `hardware` che descrive la macchina e propone miglioramenti;
- brain in RAM con `ricorda:` e `cerca`.

**Cosa ancora manca:** nel kernel non c'è ancora nessun modello di AI (oggi capisce frasi tramite parole chiave), niente disco, rete, USB, audio, multi-core, avvio UEFI su PC reale. È proprio qui che serve aiuto.

👉 **Vuoi provarlo?** Istruzioni per macchina virtuale (QEMU, Bochs) e chiavetta USB in [**docs/INSTALLAZIONE.md**](docs/INSTALLAZIONE.md).

I sorgenti del prototipo verranno pubblicati in questo repository a breve.

## La strada

| Tappa | Obiettivo | Stato |
|---|---|---|
| 0 | Kernel Rust avviabile, shell testuale, router a regole | ✅ prototipo v0.1 |
| 1 | Desktop grafico modificabile da frase, senza modello | ✅ prototipo v0.2 |
| 2 | Classificatore di intenti minuscolo, addestrato da noi, al posto delle parole chiave | 🔜 |
| 3 | Disco, file system persistente, multi-core | ⏳ |
| 4 | Modello linguistico piccolo che gira su CPU | ⏳ |
| 5 | Avvio da chiavetta su PC reale (UEFI, USB, framebuffer) | ⏳ |
| 6 | Voce: riconoscimento e risposta vocale | ⏳ |
| 7 | Brain locale persistente e multimodale; auto-apprendimento dei modelli | ⏳ |

Dettagli in [ROADMAP](docs/ROADMAP.md).

## Ti chiedo aiuto 🙏

Questo progetto è troppo grande per una persona sola, ed è giusto così: un sistema operativo che appartiene a chi lo usa deve nascere da una comunità.

Cerco persone curiose — esperte o alle prime armi — su:

- **Sviluppo OS in Rust**: UEFI, driver USB/NVMe/audio, scheduler, multi-core, file system.
- **Modelli piccoli su CPU**: inferenza `no_std`, quantizzazione, SIMD (AVX2/NEON), classificatori minuscoli.
- **Apprendimento on-device**: come può un sistema migliorare i propri modelli in sicurezza.
- **Voce**: speech-to-text e text-to-speech leggeri e locali.
- **Brain locale**: indici vettoriali, RAG, memoria episodica, privacy by design.
- **UX generativa**: come si progetta un'interfaccia che si ridisegna da sola senza disorientare.
- **Sicurezza**: sandbox per app e agenti generati, permessi, autonomia controllata.
- **Test su hardware reale**, documentazione, traduzioni, idee folli.

### Come partecipare

- 💡 **Hai un'idea?** Apri una [issue "Idea"](https://github.com/bdbais/b-ai-s/issues/new?template=idea.yml) o scrivi nelle [Discussions](https://github.com/bdbais/b-ai-s/discussions).
- 🛠️ **Vuoi contribuire?** Leggi [CONTRIBUTING](CONTRIBUTING.md).
- ❓ **Hai una domanda aperta da proporre?** Le grandi questioni irrisolte sono in [docs/DOMANDE-APERTE.md](docs/DOMANDE-APERTE.md): rispondi, critica, proponi.
- ⭐ **Ti piace il sogno?** Una stella al repo aiuta a farlo conoscere.

Partecipando accetti il [Codice di condotta](CODE_OF_CONDUCT.md).

---

## 🇬🇧 English

**B-AI-S is a generative operating system**: a kernel that boots an AI and treats it as the heart of the machine. Every sentence becomes what you expect — a redesigned desktop, a brand-new app, a server, a new agent. Everything you do feeds a **local brain** that gets to know you, and **never leaves your machine** unless you decide so. It installs from a **USB stick**.

**Principles:** local-first · the interface is a conversation · many small models, not one giant · it learns, but asks first · it improves itself, transparently · it knows its own hardware · small and readable.

**Where we are:** a bare-metal Rust (x86_64) kernel prototype that boots in a VM with a kernel-drawn graphical desktop you can restyle with a sentence, a habit-detection seed, and an in-RAM brain. There is **no AI model in the kernel yet** (intent routing is keyword-based), no disk, network, USB, audio, SMP or real-hardware UEFI boot. That's where we need you. **Try it** in a VM or from a USB stick: see [docs/INSTALLAZIONE.md](docs/INSTALLAZIONE.md) (English summary at the bottom).

**We're looking for help with:** Rust OS development (UEFI, drivers, SMP, file systems), small models on CPU (`no_std` inference, quantization, SIMD), on-device learning, local speech, vector stores and RAG, generative UX, sandboxing and security, real-hardware testing, docs and translations — and wild ideas.

Read the [Manifesto](docs/MANIFESTO.md) (Italian + English), open an [Idea issue](https://github.com/bdbais/b-ai-s/issues/new?template=idea.yml), join the [Discussions](https://github.com/bdbais/b-ai-s/discussions), or see [CONTRIBUTING](CONTRIBUTING.md). Issues and PRs in English are very welcome.

---

Licenza / License: [MIT](LICENSE) · Ideato da [Bais](https://bais.info)
