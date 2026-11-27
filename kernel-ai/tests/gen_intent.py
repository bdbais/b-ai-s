#!/usr/bin/env python3
"""bais-intent: classificatore degli intenti del desktop per kernel-ai (Fase 3).

Un llama minuscolo, a caratteri, addestrato qui da zero: legge una frase (minuscole, senza
accenti, come la normalizza il kernel) e scrive le etichette del tema, per esempio
«vorrei un desktop tipo mac al buio» -> «#macos #scuro». Per le frasi che non riguardano il
tema scrive «#altro». Il kernel applica le etichette direttamente al tema.

uso: gen_intent.py <out.gguf> [passi]      (richiede numpy, autograd, gguf)
Stampa la precisione sull'insieme di prova (frasi mai viste in addestramento) e scrive
tests/intent_test.tsv con le frasi di prova e le etichette attese.
"""
import os
import random
import sys
import time

import autograd.numpy as anp
import numpy as np
from autograd import grad

out = sys.argv[1]
STEPS = int(sys.argv[2]) if len(sys.argv) > 2 else 3000
rnd = random.Random(7)

# ---------------------------------------------------------------- etichette e frasi
COLORS = ["viola", "blu", "azzurro", "verde", "turchese", "rosso", "arancione", "giallo", "rosa", "grigio", "nero", "bianco"]
STYLE = {
    "macos": ["macos", "mac", "stile mac", "stile macos", "come un mac", "tipo mac", "alla apple", "stile apple", "come il mac", "da mac"],
    "windows": ["windows", "windows 11", "win11", "stile windows", "come windows", "tipo windows", "da pc windows", "stile windows 11"],
    "mix": ["mix", "un mix di mac e windows", "misto mac e windows", "stile mix"],
    "bais": ["ripristina", "stile b-ai-s", "tema predefinito", "quello originale", "tema di base", "riportalo all'originale"],
}
GROUPS = [
    ("tema", {
        "scuro": ["scuro", "dark", "di notte", "modalita notte", "modalita notturna", "al buio", "tema scuro", "colori scuri", "nero come la notte"],
        "chiaro": ["chiaro", "light", "di giorno", "modalita giorno", "luminoso", "tema chiaro", "colori chiari"],
    }),
    ("pulsanti", {
        "btn_dx": ["pulsanti a destra", "bottoni a destra", "i tasti della finestra a destra", "i pulsanti sulla destra"],
        "btn_sx": ["pulsanti a sinistra", "bottoni a sinistra", "i tasti della finestra a sinistra", "i pulsanti sulla sinistra"],
    }),
    ("titoli", {
        "tit_centro": ["titoli centrati", "titolo centrato", "titoli al centro", "il titolo in mezzo"],
        "tit_sx": ["titoli a sinistra", "titolo a sinistra", "il titolo allineato a sinistra"],
    }),
    ("angoli", {
        "angoli_0": ["angoli squadrati", "angoli retti", "senza angoli arrotondati", "spigoli vivi", "finestre squadrate"],
        "angoli_11": ["angoli arrotondati", "angoli tondi", "bordi arrotondati", "finestre arrotondate"],
        "angoli_18": ["angoli molto arrotondati", "angoli tondissimi", "bordi molto arrotondati"],
    }),
    ("accento", {f"acc_{c}": [f"accento {c}", f"colore {c}", f"accento di colore {c}", f"evidenziato in {c}", f"con il {c} come colore principale"] for c in COLORS}),
    ("sfondo", {f"sfondo_{c}": [f"sfondo {c}", f"sfondo di colore {c}", f"lo sfondo {c}", f"uno sfondo {c}", f"sfondo tutto {c}"] for c in COLORS}),
    ("menu", {
        "menu_si": ["barra dei menu", "con la barra dei menu", "metti la barra dei menu", "barra in alto"],
        "menu_no": ["senza barra dei menu", "togli la barra dei menu", "nascondi la barra dei menu", "niente barra in alto"],
    }),
    ("ombre", {
        "ombre_si": ["ombre", "con le ombre", "metti le ombre", "ombre sotto le finestre"],
        "ombre_no": ["senza ombre", "togli le ombre", "niente ombre", "piatto senza ombre"],
    }),
    ("barra", {
        "taskbar": ["la barra di windows", "barra di windows", "la barra delle applicazioni", "taskbar", "la barra in basso di windows"],
        "dock": ["il dock", "il dock di macos", "dock", "il dock come sul mac"],
    }),
]
ORDER = ["tema", "pulsanti", "titoli", "angoli", "accento", "sfondo", "menu", "ombre", "barra"]
PREFIX = ["", "", "", "fammi ", "voglio ", "vorrei ", "metti ", "imposta ", "rendi il desktop ", "mi piacerebbe ", "per favore ", "cambia in ", "desktop ", "interfaccia ", "un desktop "]
JOIN = [" ", " e ", " con ", ", ", " e poi ", " ma con ", " con anche "]
OTHER = [
    "che ore sono", "come stai", "raccontami una barzelletta", "quanto fa due piu due", "scrivi una poesia",
    "dove sono i miei file", "spegni tutto", "che tempo fa domani", "chi sei", "accendi la luce", "ciao",
    "grazie mille", "come ti chiami", "apri la posta", "fai una ricerca sul meteo", "traduci in inglese",
    "quanti anni hai", "ricordami di comprare il latte", "suona una canzone", "elenca i processi",
    "cosa sai fare", "calcola la media", "mostra le note", "chiama mario", "prenota un tavolo",
    "il mio colore preferito e il verde", "ho comprato un mac nuovo", "windows si e bloccato al lavoro",
    "la notte scorsa ho dormito poco", "le ombre del pomeriggio sono lunghe", "il cielo e azzurro oggi",
]


def theme_example():
    """Frase sul tema: uno stile e/o da 1 a 3 modifiche, nell'ordine o mescolate."""
    tags, parts = [], []
    if rnd.random() < 0.55:
        k = rnd.choice(list(STYLE))
        tags.append(k)
        parts.append(rnd.choice(STYLE[k]))
    n = rnd.choice([0, 1, 1, 2, 2, 3]) if tags else rnd.choice([1, 1, 2, 2, 3])
    for gname, opts in rnd.sample(GROUPS, n):
        k = rnd.choice(list(opts))
        tags.append((gname, k))
        parts.append(rnd.choice(opts[k]))
    style = [t for t in tags if isinstance(t, str)]
    mods = sorted([t for t in tags if not isinstance(t, str)], key=lambda t: ORDER.index(t[0]))
    lab = style + [m[1] for m in mods]
    if len(parts) > 1 and rnd.random() < 0.3:
        first, rest = parts[0], parts[1:]
        rnd.shuffle(rest)
        parts = [first] + rest
    text = parts[0]
    for p in parts[1:]:
        text += rnd.choice(JOIN) + p
    return rnd.choice(PREFIX) + text, lab


# frasi estranee al tema per l'addestramento, generate a combinazioni; le frasi di OTHER
# restano solo per la prova. Alcune contengono apposta parole del tema in un altro senso.
VERB = ["apri", "cerca", "mostra", "scrivi", "calcola", "ricorda", "chiama", "prenota", "traduci", "spiega",
        "dimmi", "leggi", "manda", "elenca", "controlla", "salva", "stampa", "copia", "riassumi", "prepara"]
OBJ = ["la posta", "il meteo", "una ricetta", "giulia", "le note", "il file", "un messaggio", "la lista della spesa",
       "il calendario", "le foto", "un documento", "il conto", "la mappa", "gli appunti", "il treno per milano",
       "una mail a luca", "il programma", "i processi", "la cartella dei progetti", "un promemoria"]
ASK = ["che giorno e oggi", "quanto manca a natale", "come si dice gatto in inglese", "perche il cielo e blu",
       "quanto pesa la luna", "chi ha vinto ieri", "come funziona un motore", "cosa mangio stasera",
       "qual e la capitale della francia", "mi senti", "sei sveglio", "ci sei", "buongiorno", "buonasera",
       "va tutto bene", "che cosa sei", "aiutami con un esercizio", "sono stanco", "andiamo", "perfetto"]
THING = ["mac", "pc windows", "telefono", "computer", "monitor", "tablet"]
MOOD = ["ho comprato un {t} nuovo", "il mio {t} non si accende", "mio fratello usa un {t}", "ho rotto il {t}",
        "quanto costa un {t}", "il {t} dell'ufficio e lento", "vendo il mio vecchio {t}"]
NATURE = ["il {n} e {c} oggi", "ho visto un {n} {c}", "mi piace il {n} quando e {c}", "il {n} era tutto {c}"]
NOUN = ["cielo", "mare", "prato", "fiore", "gatto", "vestito", "tramonto", "muro"]
TIME = ["stanotte era buio pesto", "di notte non dormo", "oggi e una giornata chiara", "domani e giorno di festa",
        "le ombre si allungano la sera", "al buio ho paura", "la notte porta consiglio", "che bella giornata luminosa",
        "ho lasciato le chiavi a destra del tavolo", "il titolo del libro e lungo", "l'angolo della stanza e umido",
        "la barra dei cereali e finita", "il menu del ristorante e buono", "il dock del porto e chiuso"]


def other_example(test=False):
    if test:
        return rnd.choice(OTHER), ["altro"]
    r = rnd.random()
    if r < 0.35:
        t = rnd.choice(VERB) + " " + rnd.choice(OBJ)
    elif r < 0.55:
        t = rnd.choice(ASK)
    elif r < 0.7:
        t = rnd.choice(MOOD).format(t=rnd.choice(THING))
    elif r < 0.85:
        t = rnd.choice(NATURE).format(n=rnd.choice(NOUN), c=rnd.choice(COLORS))
    else:
        t = rnd.choice(TIME)
    if rnd.random() < 0.2:
        t = rnd.choice(["per favore ", "senti, ", "ehi ", "scusa, "]) + t
    return t, ["altro"]


def example(test=False):
    return other_example(test) if rnd.random() < 0.2 else theme_example()


# frasi di prova fisse (mai usate in addestramento) + quelle del criterio della Fase 3
FIXED = [
    ("stile macos scuro", ["macos", "scuro"]),
    ("macos scuro con la barra di windows e accento viola", ["macos", "scuro", "acc_viola", "taskbar"]),
    ("vorrei un desktop tipo mac al buio", ["macos", "scuro"]),
    ("fammi windows chiaro senza ombre", ["windows", "chiaro", "ombre_no"]),
    ("che ore sono", ["altro"]),
]
seen = set()
test = list(FIXED)
for t, _ in FIXED:
    seen.add(t)
while len(test) < 300:
    t, l = example(test=True)
    if t not in seen:
        seen.add(t)
        test.append((t, l))
train_pool = []
while len(train_pool) < 40000:
    t, l = example()
    if t not in seen:
        train_pool.append((t, l))

# ---------------------------------------------------------------- vocabolario (SentencePiece a caratteri)
CHARS = list("abcdefghijklmnopqrstuvwxyz0123456789'.,!?-:")
TAGS = ["macos", "windows", "mix", "bais"] + [k for _, o in GROUPS for k in o] + ["altro"]
tokens = ["<unk>", "<s>", "</s>", "▁"] + CHARS + ["▁#" + t for t in TAGS]
scores = [0.0, 0.0, 0.0, -1.0] + [-2.0] * len(CHARS) + [-3.0] * len(TAGS)
types = [2, 3, 3] + [1] * (len(tokens) - 3)
for b in range(256):
    tokens.append("<0x%02X>" % b)
    scores.append(0.0)
    types.append(6)
V = len(tokens)
tid = {t: i for i, t in enumerate(tokens)}
BOS, EOS = 1, 2


def encode(text):
    """Come llama.cpp per questo vocabolario: <s>, spazio iniziale, un token per carattere."""
    ids = [BOS]
    for ch in " " + text:
        ids.append(tid["▁"] if ch == " " else tid.get(ch, 0))
    return ids


def tag_ids(lab):
    return [tid["▁#" + t] for t in lab] + [EOS]


# ---------------------------------------------------------------- modello (llama come in llama.cpp)
D, H, L, FF, CTX = 96, 4, 3, 192, 128
HD = D // H
rng = np.random.default_rng(11)


def init():
    s = 1 / np.sqrt(D)
    p = {"tok": rng.standard_normal((V, D)) * 0.3, "out_norm": np.ones(D), "out": rng.standard_normal((V, D)) * s}
    for i in range(L):
        p[f"{i}.an"] = np.ones(D)
        p[f"{i}.q"] = rng.standard_normal((D, D)) * s
        p[f"{i}.k"] = rng.standard_normal((D, D)) * s
        p[f"{i}.v"] = rng.standard_normal((D, D)) * s
        p[f"{i}.o"] = rng.standard_normal((D, D)) * s / np.sqrt(2 * L)
        p[f"{i}.fn"] = np.ones(D)
        p[f"{i}.g"] = rng.standard_normal((FF, D)) * s
        p[f"{i}.u"] = rng.standard_normal((FF, D)) * s
        p[f"{i}.d"] = rng.standard_normal((D, FF)) * (1 / np.sqrt(FF)) / np.sqrt(2 * L)
    return {k: v.astype(np.float32) for k, v in p.items()}


# RoPE «normale» di llama.cpp (LLAMA_ROPE_TYPE_NORM): coppie adiacenti (2i, 2i+1)
inv = 10000.0 ** (-np.arange(0, HD, 2) / HD)
ang = np.arange(CTX)[:, None] * inv[None, :]
COS, SIN = np.cos(ang).astype(np.float32), np.sin(ang).astype(np.float32)


def rope(x, T):  # x: B,T,H,HD
    x0, x1 = x[..., 0::2], x[..., 1::2]
    c, s = COS[None, :T, None, :], SIN[None, :T, None, :]
    r0, r1 = x0 * c - x1 * s, x0 * s + x1 * c
    return anp.reshape(anp.stack([r0, r1], axis=-1), x.shape)


def rms(x, w):
    return x / anp.sqrt(anp.mean(x * x, axis=-1, keepdims=True) + np.float32(1e-5)) * w


def forward(p, ids):
    B, T = ids.shape
    x = p["tok"][ids]
    mask = np.triu(np.full((T, T), -1e9, dtype=np.float32), 1)
    for i in range(L):
        h = rms(x, p[f"{i}.an"])
        q = anp.reshape(anp.dot(h, p[f"{i}.q"].T), (B, T, H, HD))
        k = anp.reshape(anp.dot(h, p[f"{i}.k"].T), (B, T, H, HD))
        v = anp.reshape(anp.dot(h, p[f"{i}.v"].T), (B, T, H, HD))
        q = anp.transpose(rope(q, T), (0, 2, 1, 3))
        k = anp.transpose(rope(k, T), (0, 2, 3, 1))
        v = anp.transpose(v, (0, 2, 1, 3))
        a = anp.matmul(q, k) * np.float32(1 / np.sqrt(HD)) + mask
        a = a - anp.max(a, axis=-1, keepdims=True)
        a = anp.exp(a)
        a = a / anp.sum(a, axis=-1, keepdims=True)
        o = anp.reshape(anp.transpose(anp.matmul(a, v), (0, 2, 1, 3)), (B, T, D))
        x = x + anp.dot(o, p[f"{i}.o"].T)
        h = rms(x, p[f"{i}.fn"])
        g = anp.dot(h, p[f"{i}.g"].T)
        u = anp.dot(h, p[f"{i}.u"].T)
        x = x + anp.dot(g / (1 + anp.exp(-g)) * u, p[f"{i}.d"].T)
    return anp.dot(rms(x, p["out_norm"]), p["out"].T)


def batch(exs):
    seqs, ws = [], []
    for t, l in exs:
        a, b = encode(t), tag_ids(l)
        s = (a + b)[:CTX]
        seqs.append(s)
        ws.append([0] * (len(a) - 1) + [1] * len(b))
    T = max(len(s) for s in seqs)
    ids = np.zeros((len(seqs), T), dtype=np.int64)
    tgt = np.zeros((len(seqs), T - 1), dtype=np.int64)
    w = np.zeros((len(seqs), T - 1), dtype=np.float32)
    for j, (s, m) in enumerate(zip(seqs, ws)):
        ids[j, : len(s)] = s
        tgt[j, : len(s) - 1] = s[1:]
        w[j, : len(m)] = m[: len(s) - 1]
    return ids, tgt, w


def loss(p, ids, tgt, w):
    lg = forward(p, ids[:, :-1])
    lg = lg - anp.max(lg, axis=-1, keepdims=True)
    lp = lg - anp.log(anp.sum(anp.exp(lg), axis=-1, keepdims=True))
    B, T = tgt.shape
    pick = anp.reshape(lp, (B * T, V))[np.arange(B * T), tgt.reshape(-1)]
    return -anp.sum(pick * w.reshape(-1)) / np.sum(w)


g_loss = grad(loss)


def predict(p, text, maxn=8):
    ids = encode(text)
    lab = []
    for _ in range(maxn):
        lg = forward(p, np.array([ids]))[0, -1]
        t = int(np.argmax(lg))
        if t == EOS:
            break
        lab.append(tokens[t][2:] if tokens[t].startswith("▁#") else tokens[t])
        ids.append(t)
    return lab


p = init()
m = {k: np.zeros_like(v) for k, v in p.items()}
vv = {k: np.zeros_like(v) for k, v in p.items()}
lr0, b1, b2 = 3e-3, 0.9, 0.98
t0 = time.time()
for step in range(1, STEPS + 1):
    exs = rnd.sample(train_pool, 32)
    ids, tgt, w = batch(exs)
    g = g_loss(p, ids, tgt, w)
    lr = lr0 * min(1.0, step / 200) * (0.1 + 0.9 * 0.5 * (1 + np.cos(np.pi * step / STEPS)))
    for k in p:
        m[k] = b1 * m[k] + (1 - b1) * g[k]
        vv[k] = b2 * vv[k] + (1 - b2) * g[k] ** 2
        p[k] -= np.float32(lr) * (m[k] / (1 - b1**step)) / (np.sqrt(vv[k] / (1 - b2**step)) + 1e-8)
    if step % 250 == 0 or step == 1:
        print(f"passo {step} perdita {loss(p, ids, tgt, w):.4f} ({time.time() - t0:.0f} s)", flush=True)

np.savez(out + ".npz", **p)
ok = 0
bad = []
for t, l in test:
    pr = predict(p, t)
    if pr == l:
        ok += 1
    else:
        bad.append((t, l, pr))
print(f"insieme di prova: {ok}/{len(test)} esatte ({100 * ok / len(test):.1f}%)")
for b in bad[:15]:
    print("  ERR", b)
for t, l in FIXED:
    print(f"  {t!r} -> {' '.join('#' + x for x in predict(p, t))}")
with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "intent_test.tsv"), "w") as f:
    for t, l in test:
        f.write(t + "\t" + " ".join("#" + x for x in l) + "\n")

# ---------------------------------------------------------------- esportazione GGUF (F32)
import gguf  # noqa: E402

wr = gguf.GGUFWriter(out, "llama")
wr.add_name("bais-intent")
wr.add_description("classificatore degli intenti del desktop di kernel-ai (B-AI-S), addestrato da tests/gen_intent.py")
wr.add_string("bais.role", "intent")
wr.add_context_length(CTX)
wr.add_embedding_length(D)
wr.add_block_count(L)
wr.add_feed_forward_length(FF)
wr.add_head_count(H)
wr.add_head_count_kv(H)
wr.add_rope_dimension_count(HD)
wr.add_rope_freq_base(10000.0)
wr.add_layer_norm_rms_eps(1e-5)
wr.add_file_type(gguf.LlamaFileType.ALL_F32)
wr.add_tokenizer_model("llama")
wr.add_token_list(tokens)
wr.add_token_scores(scores)
wr.add_token_types(types)
wr.add_bos_token_id(BOS)
wr.add_eos_token_id(EOS)
wr.add_unk_token_id(0)
wr.add_add_bos_token(True)
wr.add_add_eos_token(False)
f32 = lambda a: np.ascontiguousarray(a, dtype=np.float32)  # noqa: E731
wr.add_tensor("token_embd.weight", f32(p["tok"]))
for i in range(L):
    b = f"blk.{i}."
    wr.add_tensor(b + "attn_norm.weight", f32(p[f"{i}.an"]))
    wr.add_tensor(b + "attn_q.weight", f32(p[f"{i}.q"]))
    wr.add_tensor(b + "attn_k.weight", f32(p[f"{i}.k"]))
    wr.add_tensor(b + "attn_v.weight", f32(p[f"{i}.v"]))
    wr.add_tensor(b + "attn_output.weight", f32(p[f"{i}.o"]))
    wr.add_tensor(b + "ffn_norm.weight", f32(p[f"{i}.fn"]))
    wr.add_tensor(b + "ffn_gate.weight", f32(p[f"{i}.g"]))
    wr.add_tensor(b + "ffn_up.weight", f32(p[f"{i}.u"]))
    wr.add_tensor(b + "ffn_down.weight", f32(p[f"{i}.d"]))
wr.add_tensor("output_norm.weight", f32(p["out_norm"]))
wr.add_tensor("output.weight", f32(p["out"]))
wr.write_header_to_file()
wr.write_kv_data_to_file()
wr.write_tensors_to_file()
wr.close()
print("scritto", out, V, "token")
