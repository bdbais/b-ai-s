# Domande aperte / Open questions

Le grandi questioni che non abbiamo ancora risolto. Ogni risposta, critica o controproposta è preziosa: apri una [Discussion](https://github.com/bdbais/b-ai-s/discussions) o una issue citando il numero della domanda.

*The big unsolved questions. Answers, critiques and counter-proposals are all welcome — open a Discussion or an issue quoting the question number.*

1. **Dove vive il modello?** Runtime di inferenza dentro il kernel (`no_std`), oppure kernel minimo + userspace che ospita un runtime esistente? Quali compromessi su sicurezza, semplicità e prestazioni?
   *Where does the model live — inside the kernel or in a userspace on top of a minimal kernel?*

2. **Quanto piccolo può essere il classificatore di intenti** per capire bene frasi libere in più lingue? Quale architettura (n-gram, piccolo transformer, embedding + kNN)?
   *How tiny can a multilingual intent classifier be?*

3. **Come si addestra on-device in sicurezza?** Come evitare che il sistema "impari male" o peggiori? Come si rende ogni cambiamento reversibile e spiegabile?
   *How do we do safe, reversible on-device learning?*

4. **Come si mette in sandbox un'app generata** in un sistema senza un modello di processi tradizionale? Che permessi, che capability?
   *How do we sandbox generated apps and agents?*

5. **Interfaccia che si ridisegna da sola:** come evitare di disorientare l'utente? Che limiti, che "memoria" dell'aspetto?
   *How does a self-redesigning UI avoid disorienting users?*

6. **Autonomia controllata:** quando il sistema può agire da solo? Come si chiede il permesso senza diventare fastidioso?
   *When may the system act on its own, and how does it ask without nagging?*

7. **Formato del brain locale:** come conservare testi, immagini, audio e video in un indice unico, cifrato, portabile su chiavetta?
   *What format for an encrypted, portable, multimodal local brain?*

8. **Hardware di riferimento:** su quali macchine puntare per primo (PC x86 con AVX2? ARM? Raspberry Pi?)
   *Which reference hardware first?*

Hai una domanda che manca? Aggiungila con una pull request.
