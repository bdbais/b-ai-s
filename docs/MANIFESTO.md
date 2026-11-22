# Il Manifesto di B-AI-S

🇮🇹 Italiano · [🇬🇧 English](#-the-b-ai-s-manifesto)

## Perché

Per quarant'anni abbiamo adattato noi stessi ai computer. Abbiamo imparato dove stanno i menu, come si chiamano le cartelle, quale app apre quale file. Abbiamo consegnato i nostri dati a servizi lontani in cambio di comodità.

L'intelligenza artificiale permette di rovesciare il rapporto: **è il computer che deve imparare noi**. Ma se questo apprendimento avviene sui server di qualcun altro, abbiamo solo cambiato padrone.

B-AI-S nasce da un'idea semplice: **un sistema operativo il cui nucleo è un'intelligenza che vive sulla tua macchina, impara da te e lavora per te.**

## Cos'è

B-AI-S è un **boot loader per un modello**: un kernel che avvia l'AI e le dà il controllo della macchina, come un sistema operativo. Non esistono app installate nel senso tradizionale: esistono **richieste**, e il sistema trasforma ogni richiesta in ciò che ti serve.

- Un'**interfaccia**: «stile macOS», «stile Windows 11», «un mix scuro con accento viola». Il desktop è un'opinione, non un vincolo.
- Un'**applicazione**: generata sul momento, in una sandbox, e migliorata con il tuo feedback.
- Un **servizio**: il sistema può diventare un server per qualcosa.
- Un **agente**: un nuovo organo del sistema, specializzato, che comunica con gli altri.
- Una **risposta**: a voce, in grafica, o azionando un dispositivo collegato.

## I sette principi

### 1. Locale prima di tutto
Tutto — testi, documenti, immagini, video, ricordi, modelli — finisce in un **brain locale**. Nulla esce dalla macchina senza una tua decisione esplicita. Il cloud può esistere come aiuto opzionale, ma il sistema deve funzionare senza.

### 2. L'interfaccia è una conversazione
Si comanda con il linguaggio: scritto, parlato, in futuro anche con lo sguardo e i gesti. L'interfaccia stessa si modifica con una frase, e una frase può annullare la modifica.

### 3. Tanti modelli piccoli, non uno gigante
Un classificatore minuscolo capisce cosa vuoi; un modello specializzato lo realizza; agenti locali fanno da servizi e da organi di comunicazione. Il sistema **sceglie** il modello giusto per ogni compito e impara quale funziona meglio. Le prestazioni non sono la priorità iniziale: lo sono la comprensibilità e la capacità di migliorare.

### 4. Impara, ma chiede
Tutto ciò che fai, dici e vedi alimenta il "second brain" del sistema, che ti conosce sempre meglio. Quando riconosce un lavoro ripetitivo, **la prima volta chiede** se può farlo da solo. L'autonomia si guadagna, non si prende.

### 5. Si migliora da solo, in modo trasparente
B-AI-S addestra e affina continuamente i propri modelli locali con i tuoi feedback. Ogni cambiamento è visibile, spiegabile e reversibile.

### 6. Conosce il proprio corpo
Il sistema legge l'hardware su cui gira — CPU, memoria, istruzioni disponibili, schermo, periferiche — e propone come ottimizzarsi.

### 7. Piccolo e comprensibile
Si parte da un kernel minimo in **Rust**, bare-metal, che chiunque possa leggere e capire. Si cresce a piccoli passi verificabili.

## Cosa B-AI-S non vuole essere

- Non un assistente vocale incollato sopra un sistema operativo esistente.
- Non un client di un'AI in cloud.
- Non una scatola nera che decide al posto tuo.
- Non un prodotto che vende i tuoi dati o la tua attenzione.

## Un invito

Questo è un sogno, e un sogno di queste dimensioni non si realizza da soli. Se queste idee ti parlano — anche solo una — c'è posto per te: per scrivere codice, per criticare, per proporre, per sognare più in grande.

---

# 🇬🇧 The B-AI-S Manifesto

## Why

For forty years we've adapted ourselves to computers: learning menus, folders, which app opens which file — and handing our data to distant services in exchange for convenience.

AI lets us flip that relationship: **the computer should learn us**. But if that learning happens on someone else's servers, we've only changed masters.

B-AI-S starts from a simple idea: **an operating system whose core is an intelligence that lives on your machine, learns from you, and works for you.**

## What it is

B-AI-S is a **boot loader for a model**: a kernel that starts the AI and hands it the machine. There are no installed apps in the traditional sense — there are **requests**, and the system turns each one into what you need: an interface ("macOS style", "Windows 11", "a dark mix with a purple accent"), an application generated on the spot in a sandbox, a service, an agent (a new organ of the system), or an answer — spoken, visual, or by driving a connected device.

## The seven principles

1. **Local first.** Everything goes into a local brain. Nothing leaves the machine without your explicit decision. Cloud is optional help, never a requirement.
2. **The interface is a conversation.** Written, spoken, and someday gaze and gesture. The interface itself changes with a sentence — and a sentence can undo it.
3. **Many small models, not one giant.** A tiny classifier understands intent; specialised models carry it out; local agents act as services and communication organs. The system picks the right model per task and learns which works best.
4. **It learns, but asks.** When it spots repetitive work, it asks the first time before doing it on its own. Autonomy is earned.
5. **It improves itself, transparently.** It continuously fine-tunes its local models from your feedback; every change is visible, explainable and reversible.
6. **It knows its own body.** It reads the hardware it runs on and proposes how to optimise itself.
7. **Small and readable.** It starts from a minimal bare-metal **Rust** kernel anyone can read, and grows in small, verifiable steps.

## What B-AI-S is not

Not a voice assistant glued onto an existing OS. Not a client for a cloud AI. Not a black box deciding for you. Not a product that sells your data or attention.

## An invitation

This is a dream, and a dream this size isn't built alone. If any of these ideas speaks to you, there's room for you — to code, to critique, to propose, to dream bigger.
