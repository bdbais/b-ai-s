#!/usr/bin/env python3
"""Test di apprendimento di kernel-ai v0.3-pc in QEMU (senza finestra), via seriale.

uso: run.py <iso> <cartella-risultati> [abitudini|brain|intent ...]
Usa ../../kernel-pc/tools/qemu_drive.py. Scrive <sessione>.log e report.md.
"""
import os, re, subprocess, sys, random
HERE = os.path.dirname(os.path.abspath(__file__))
DRIVE = os.path.join(HERE, "..", "..", "kernel-pc", "tools", "qemu_drive.py")
TSV = os.path.join(HERE, "..", "intent_test.tsv")
iso, out = sys.argv[1], sys.argv[2]
which = sys.argv[3:] or ["abitudini", "brain", "intent"]
os.makedirs(out, exist_ok=True)

ABITUDINI = [
    # 1. imparare una coppia ripetuta e accettarla
    "stile macos", "apri brain", "stile windows 11", "stile macos", "apri brain",  # -> proposta
    "si",                                                                          # -> accettata
    "stile macos",                                                                 # -> esegue da sola apri brain
    # 2. coppia rifiutata: non deve tornare né partire da sola
    "scuro", "chiaro", "pulisci", "scuro", "chiaro",                               # -> proposta
    "no",
    "hardware", "scuro", "chiaro",                                                 # -> nessuna proposta, nessuna esecuzione
    # 3. proposta ignorata (si risponde con un altro comando)
    "accento viola", "sfondo verde", "temperatura", "accento viola", "sfondo verde",  # -> proposta
    "diario",                                                                      # ignora la domanda
    "accento viola", "sfondo verde",                                               # ?
    # 4. un solo passaggio non basta
    "titoli centrati", "angoli squadrati",
    "abitudini",
]
BRAIN = [
    "ricorda: il router di casa ha indirizzo 192.168.1.1",
    "ricorda: la riunione con il capo e giovedi alle 10",
    "ricorda: Bais preferisce il tema scuro",
    "cerca router", "cerca riunione capo", "cerca ROUTER", "cerca tema scuro",
    "cerca appuntamento", "cerca indirizzo del modem", "cerca il",
    "dimentica tutto", "cerca router",
]
NUOVE = [  # frasi mai viste: sinonimi, ordine diverso, refusi, maiuscole, inglese, trappole
    ("tema notturno in stile apple", "#macos #scuro"),
    ("mettimi windows in modalità scura", "#windows #scuro"),
    ("vorrei il look del mac", "#macos"),
    ("finestre con gli angoli tondi e lo sfondo blu", "#angoli_11 #sfondo_blu"),
    ("un'interfaccia come windows coi bottoni a sinistra", "#windows #btn_sx"),
    ("mac chiaro col dock", "#macos #chiaro #dock"),
    ("windows 11 senza ombre e con la taskbar", "#windows #ombre_no #taskbar"),
    ("voglio i colori del giorno", "#chiaro"),
    ("accento arancione e titoli in mezzo", "#acc_arancione #tit_centro"),
    ("riporta tutto com'era all'inizio", "#bais"),
    ("stile macso scuro", "#macos #scuro"),
    ("MACOS SCURO", "#macos #scuro"),
    ("dark mode stile mac", "#macos #scuro"),
    ("black theme windows", "#windows #scuro"),
    ("sfondo color rosso", "#sfondo_rosso"),
    ("evidenzia tutto in giallo", "#acc_giallo"),
    ("nascondi le ombre", "#ombre_no"),
    ("togli la barra in alto", "#menu_no"),
    ("il mio gatto è nero", "#altro"),
    ("che tempo fa a milano", "#altro"),
    ("accendi la musica", "#altro"),
    ("prenota un tavolo per due", "#altro"),
    ("mi passi il sale", "#altro"),
    ("ho sognato un mac viola", "#altro"),
    ("il cielo stanotte è scuro", "#altro"),
    ("domani compro windows", "#altro"),
    ("spegni il computer", "#altro"),
    ("quanto fa due più due", "#altro"),
]

def norm_tags(s):
    return frozenset(t for t in s.split() if t.startswith("#"))

def run(name, lines):
    log = os.path.join(out, name + ".log")
    env = dict(os.environ, QUIET=os.environ.get("QUIET", "4"), MAXS="240")
    subprocess.run([sys.executable, DRIVE, iso, log] + lines, env=env, stdout=subprocess.DEVNULL, check=False)
    return open(log, encoding="utf-8", errors="replace").read()

def blocks(text):
    """>>> frase -> uscita"""
    parts = re.split(r"\n>>> (.*)\n", text)
    return list(zip(parts[1::2], parts[2::2]))

rep = []
if "abitudini" in which:
    b = blocks(run("abitudini", ABITUDINI))
    o = lambda i: b[i][1]
    checks = [
        ("1a proposta dopo due ripetizioni di «stile macos → apri brain»", "Posso farlo da solo" in o(4)),
        ("1b nessuna proposta dopo una sola ripetizione", all("Posso farlo da solo" not in o(i) for i in range(4))),
        ("1c «si» accetta l'abitudine", "D'accordo" in o(5)),
        ("1d dopo «stile macos» esegue da sola «apri brain»", "abitudine: eseguo «apri brain»" in o(6)),
        ("2a proposta per «scuro → chiaro»", "Posso farlo da solo" in o(11)),
        ("2b «no» la rifiuta", "non lo farò da solo" in o(12)),
        ("2c dopo il rifiuto non viene riproposta", "Posso farlo da solo" not in o(15)),
        ("2d dopo il rifiuto non parte da sola", "abitudine: eseguo" not in o(14)),
        ("3a proposta per «accento viola → sfondo verde»", "Posso farlo da solo" in o(20)),
        ("3b proposta ignorata: viene richiesta di nuovo", "Posso farlo da solo" in o(23)),
        ("4  una coppia vista una volta sola non diventa abitudine", "titoli centrati" not in o(26)),
    ]
    rep.append(("Abitudini", checks, b[-1][1].strip()))
if "brain" in which:
    b = blocks(run("brain", BRAIN))
    o = lambda i: b[i][1]
    checks = [
        ("ricorda: tre ricordi salvati", "3 ricordi" in o(2)),
        ("cerca router → ricordo del router", "192.168.1.1" in o(3)),
        ("cerca riunione capo → riunione, 2 parole", "[2] la riunione" in o(4)),
        ("cerca ROUTER (maiuscolo) → trovato", "192.168.1.1" in o(5)),
        ("cerca tema scuro → preferenza", "tema scuro" in o(6)),
        ("cerca appuntamento (sinonimo di riunione) → riunione", "riunione" in o(7)),
        ("cerca indirizzo del modem (parafrasi) → router", "192.168.1.1" in o(8)),
        ("cerca il → nessun risultato (parola corta ignorata)", "niente di pertinente" in o(9)),
        ("dimentica tutto → brain vuoto", "niente di pertinente" in o(11)),
    ]
    rep.append(("Brain", checks, ""))
if "intent" in which:
    random.seed(7)
    base = [l.rstrip("\n").split("\t") for l in open(TSV, encoding="utf-8")][5:]
    base = random.sample(base, 12)
    cases = [("nuova", t, e) for t, e in NUOVE] + [("prova", t, e) for t, e in base]
    if os.environ.get("SLICE"):  # es. SLICE=0:20, per sessioni brevi
        a, z = (int(x) for x in os.environ["SLICE"].split(":"))
        cases = cases[a:z]
    b = blocks(run("intent" + os.environ.get("SLICE", "").replace(":", "-"), ["capisci " + t for _, t, _ in cases]))
    rows = []
    for (kind, t, e), (_, outp) in zip(cases, b):
        m = re.search(r"intento:([^\n(]*)\(", outp)
        got = m.group(1).strip() if m else "?"
        rows.append((kind, t, e, got, norm_tags(got) == norm_tags(e)))
    rep.append(("Classificatore", rows, ""))

md = ["# Test di apprendimento kernel-ai v0.3-pc (QEMU)\n"]
for title, items, extra in rep:
    md.append(f"## {title}\n")
    if title == "Classificatore":
        for k in ("nuova", "prova"):
            sub = [r for r in items if r[0] == k]
            ok = sum(r[4] for r in sub)
            md.append(f"**Frasi {k}:** {ok}/{len(sub)} esatte\n")
        md.append("| tipo | frase | attese | ottenute | ok |\n|---|---|---|---|---|")
        for r in items:
            md.append(f"| {r[0]} | {r[1]} | {r[2]} | {r[3]} | {'✅' if r[4] else '❌'} |")
    else:
        md.append(f"**{sum(c[1] for c in items)}/{len(items)} superati**\n")
        for name, ok in items:
            md.append(f"- {'✅' if ok else '❌'} {name}")
        if extra:
            md.append("\nUscita di `abitudini`:\n```\n" + extra + "\n```")
    md.append("")
open(os.path.join(out, "report.md"), "w", encoding="utf-8").write("\n".join(md))
print("\n".join(md))
