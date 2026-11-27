# Test di apprendimento — kernel-ai v0.3-pc in QEMU (2026-11-26)

Eseguiti su baisryzen7, QEMU 6.2 senza finestra (TCG, `-cpu qemu64`, 512 MB), ISO `dist/bais-kernel-v0.3-pc.iso`, frasi inviate sulla seriale da `run.py`.

## Riepilogo
- Abitudini: 10/11
- Brain: 8/9
- Classificatore, frasi mai viste: 18/28; campione dell'insieme di prova: 12/12

## Abitudini

**10/11 superati**

- ✅ 1a proposta dopo due ripetizioni di «stile macos → apri brain»
- ✅ 1b nessuna proposta dopo una sola ripetizione
- ✅ 1c «si» accetta l'abitudine
- ✅ 1d dopo «stile macos» esegue da sola «apri brain»
- ✅ 2a proposta per «scuro → chiaro»
- ✅ 2b «no» la rifiuta
- ✅ 2c dopo il rifiuto non viene riproposta
- ✅ 2d dopo il rifiuto non parte da sola
- ✅ 3a proposta per «accento viola → sfondo verde»
- ❌ 3b proposta ignorata: viene richiesta di nuovo
- ✅ 4  una coppia vista una volta sola non diventa abitudine

Uscita di `abitudini`:
```
abitudini
«stile macos» → «apri brain»  automatica
«scuro» → «chiaro»  rifiutata o in attesa
«accento viola» → «sfondo verde»  rifiutata o in attesa
>
```

## Brain

**8/9 superati**

- ✅ ricorda: tre ricordi salvati
- ✅ cerca router → ricordo del router
- ✅ cerca riunione capo → riunione, 2 parole
- ✅ cerca ROUTER (maiuscolo) → trovato
- ✅ cerca tema scuro → preferenza
- ❌ cerca appuntamento (sinonimo di riunione) → riunione
- ✅ cerca indirizzo del modem (parafrasi) → router
- ✅ cerca il → nessun risultato (parola corta ignorata)
- ✅ dimentica tutto → brain vuoto

## Classificatore

**Frasi nuove:** 18/28 · **Insieme di prova:** 12/12 · 170–300 ms a frase

| tipo | frase | attese | ottenute | ok |
|---|---|---|---|---|
| nuova | tema notturno in stile apple | #macos #scuro | #altro | ❌ |
| nuova | mettimi windows in modalità scura | #windows #scuro | #windows #scuro | ✅ |
| nuova | vorrei il look del mac | #macos | #dock | ❌ |
| nuova | finestre con gli angoli tondi e lo sfondo blu | #angoli_11 #sfondo_blu | #angoli_11 #sfondo_blu | ✅ |
| nuova | un'interfaccia come windows coi bottoni a sinistra | #windows #btn_sx | #windows #btn_sx | ✅ |
| nuova | mac chiaro col dock | #macos #chiaro #dock | #macos #chiaro #dock | ✅ |
| nuova | windows 11 senza ombre e con la taskbar | #windows #ombre_no #taskbar | #windows #ombre_no #taskbar | ✅ |
| nuova | voglio i colori del giorno | #chiaro | #chiaro #acc_nero | ❌ |
| nuova | accento arancione e titoli in mezzo | #acc_arancione #tit_centro | #tit_centro #acc_arancione | ✅ |
| nuova | riporta tutto com'era all'inizio | #bais | #bais | ✅ |
| nuova | stile macso scuro | #macos #scuro | #macos #scuro | ✅ |
| nuova | MACOS SCURO | #macos #scuro | #macos #scuro | ✅ |
| nuova | dark mode stile mac | #macos #scuro | #scuro | ❌ |
| nuova | black theme windows | #windows #scuro | #windows #chiaro #dock | ❌ |
| nuova | sfondo color rosso | #sfondo_rosso | #sfondo_rosso | ✅ |
| nuova | evidenzia tutto in giallo | #acc_giallo | #acc_giallo | ✅ |
| nuova | nascondi le ombre | #ombre_no | #ombre_si | ❌ |
| nuova | togli la barra in alto | #menu_no | #menu_no | ✅ |
| nuova | il mio gatto è nero | #altro | #altro | ✅ |
| nuova | che tempo fa a milano | #altro | #altro | ✅ |
| nuova | accendi la musica | #altro | #altro | ✅ |
| nuova | prenota un tavolo per due | #altro | #acc_viola | ❌ |
| nuova | mi passi il sale | #altro | #macos | ❌ |
| nuova | ho sognato un mac viola | #altro | #altro | ✅ |
| nuova | il cielo stanotte è scuro | #altro | #scuro #acc_nero | ❌ |
| nuova | domani compro windows | #altro | #windows | ❌ |
| nuova | spegni il computer | #altro | #altro | ✅ |
| nuova | quanto fa due più due | #altro | #altro | ✅ |
| prova | per favore spigoli vivi | #angoli_0 | #angoli_0 | ✅ |
| prova | fammi metti la barra dei menu, bordi molto arrotondati | #angoli_18 #menu_si | #angoli_18 #menu_si | ✅ |
| prova | cambia in stile b-ai-s ma con il dock come sul mac, finestre squadrate | #bais #angoli_0 #dock | #bais #angoli_0 #dock | ✅ |
| prova | per favore quello originale, bottoni a destra con anche titolo centrato | #bais #btn_dx #tit_centro | #bais #btn_dx #tit_centro | ✅ |
| prova | voglio riportalo all'originale barra in alto | #bais #menu_si | #bais #menu_si | ✅ |
| prova | cambia in senza angoli arrotondati | #angoli_0 | #angoli_0 | ✅ |
| prova | un desktop ripristina sfondo tutto rosso | #bais #sfondo_rosso | #bais #sfondo_rosso | ✅ |
| prova | metti stile apple, uno sfondo turchese | #macos #sfondo_turchese | #macos #sfondo_turchese | ✅ |
| prova | desktop stile windows 11 con nero come la notte i pulsanti sulla sinistra | #windows #scuro #btn_sx | #windows #scuro #btn_sx | ✅ |
| prova | un mix di mac e windows e poi i pulsanti sulla sinistra e poi sfondo di colore rosa | #mix #btn_sx #sfondo_rosa | #mix #btn_sx #sfondo_rosa | ✅ |
| prova | al buio | #scuro | #scuro | ✅ |
| prova | metti angoli molto arrotondati con anche sfondo di colore grigio | #angoli_18 #sfondo_grigio | #angoli_18 #sfondo_grigio | ✅ |
