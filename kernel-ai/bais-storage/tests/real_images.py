#!/usr/bin/env python3
"""Collaudo di bais-storage su immagini exFAT vere.

Le immagini le crea un'implementazione indipendente (mkfs.exfat di exfatprogs,
file scritti con exfat-fuse); il lettore del kernel deve vedere esattamente
gli stessi file, dimensioni e contenuti (sha256 confrontato con hashlib).

Serve root (losetup, mount FUSE). Uso:
    sudo python3 tests/real_images.py [--big GB] [--work DIR]
"""
import argparse, hashlib, os, random, subprocess as sp, sys, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TOOL = os.path.join(ROOT, "target", "release", "examples", "bais-disk")
ok_count = 0
fail_count = 0


def run(*a, check=True, **kw):
    return sp.run(a, check=check, capture_output=True, text=True, **kw)


def check(cond, what):
    global ok_count, fail_count
    if cond:
        ok_count += 1
        print(f"  ok    {what}")
    else:
        fail_count += 1
        print(f"  ERRORE {what}")


def tool(*a):
    r = sp.run([TOOL, *map(str, a)], capture_output=True)
    return r.returncode, r.stdout, r.stderr.decode()


class Mounted:
    """Monta un volume exFAT con exfat-fuse da un loop device."""

    def __init__(self, img, part=None, sector=512):
        self.img, self.part, self.sector = img, part, sector

    def __enter__(self):
        args = ["losetup", "-f", "--show", f"--sector-size={self.sector}"]
        if self.part:
            # Niente udev nel contenitore: la partizione diventa un loop con offset.
            import json
            t = json.loads(run("sfdisk", "-J", self.img).stdout)["partitiontable"]
            p = t["partitions"][self.part - 1]
            ss = t.get("sectorsize", 512)
            args += ["-o", str(p["start"] * ss), "--sizelimit", str(p["size"] * ss)]
        self.loop = run(*args, self.img).stdout.strip()
        dev = self.loop
        self.dir = self.img + ".mnt"
        os.makedirs(self.dir, exist_ok=True)
        self.dev = dev
        return self

    def mkfs(self, label, extra=()):
        run("mkfs.exfat", "-L", label, *extra, self.dev)

    def mount(self):
        run("mount.exfat-fuse", self.dev, self.dir)
        return self.dir

    def umount(self):
        run("umount", self.dir)

    def __exit__(self, *e):
        run("umount", self.dir, check=False)
        run("losetup", "-d", self.loop, check=False)


def rnd_bytes(seed, n):
    r = random.Random(seed)
    return r.randbytes(n)


def expected_tree(mnt):
    out = []
    for dp, dns, fns in os.walk(mnt):
        rel = os.path.relpath(dp, mnt)
        rel = "" if rel == "." else "/" + rel
        for d in dns:
            out.append(("d", 0, f"{rel}/{d}"))
        for f in fns:
            out.append(("f", os.path.getsize(os.path.join(dp, f)), f"{rel}/{f}"))
    return sorted(out)


def sha_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        while b := f.read(8 << 20):
            h.update(b)
    return h.hexdigest()


def populate_small(mnt, seed):
    """Cartelle annidate, nomi italiani e lunghi, file vuoti, file a cavallo dei cluster."""
    r = random.Random(seed)
    os.makedirs(f"{mnt}/modelli/piccoli", exist_ok=True)
    os.makedirs(f"{mnt}/Diario di bordo/2026/novembre", exist_ok=True)
    files = {
        "leggimi.txt": b"B-AI-S: chiavetta di prova\n",
        "Perché è così.txt": "àèéìòù ÀÈÉÌÒÙ".encode() * 50,
        "vuoto.bin": b"",
        "modelli/piccoli/bais-intent.gguf": rnd_bytes(seed + 1, 344_000 * 4 + 123),
        "modelli/bais-test-llama.gguf": rnd_bytes(seed + 2, 1_500_000),
        "Diario di bordo/2026/novembre/27.md": b"notte: Fase 4, lettore exFAT\n" * 1000,
        "emoji 😀 e greco αβγ.txt": b"surrogati UTF-16\n",
        ("n" * 200 + ".txt"): b"nome lungo 204 caratteri\n",
    }
    for i in range(300):  # tante voci: la cartella occupa più cluster
        files[f"molti/file-{i:03d}.dat"] = rnd_bytes(seed * 1000 + i, r.randint(0, 9000))
    for p, data in files.items():
        os.makedirs(os.path.dirname(f"{mnt}/{p}") or mnt, exist_ok=True)
        with open(f"{mnt}/{p}", "wb") as f:
            f.write(data)


def populate_fragmented(mnt, seed):
    """Due file scritti a pezzi alternati: i cluster si intrecciano (catena FAT)."""
    a, b = f"{mnt}/frammentato-a.bin", f"{mnt}/frammentato-b.bin"
    for i in range(64):
        for p, s in ((a, 1), (b, 2)):
            with open(p, "ab") as f:
                f.write(rnd_bytes(seed * 100 + i * 3 + s, 40_000 + i * 17))
    with open(f"{mnt}/contiguo.bin", "wb") as f:
        f.write(rnd_bytes(seed + 9, 3_000_000))


def verify(img, part, mnt_ctx, label, bs_list=(512,), ranges=True):
    """Confronta albero, sha256 e letture a caso tra exfat-fuse e bais-disk."""
    mnt = mnt_ctx.mount()
    exp = expected_tree(mnt)
    shas = {p: sha_file(mnt + p) for t, _, p in exp if t == "f"}
    raw = {}
    if ranges:
        for t, size, p in exp:
            if t == "f" and size > 0 and len(raw) < 40:
                with open(mnt + p, "rb") as f:
                    raw[p] = f.read()
    mnt_ctx.umount()
    for bs in bs_list:
        print(f"[{label}] blocco del dispositivo {bs} byte")
        code, out, err = tool("tree", img, bs)
        got = sorted((t, int(s), p) for t, s, p in (l.split("\t")[:3] for l in out.decode().splitlines()))
        check(code == 0 and got == exp, f"albero identico ({len(exp)} voci) {err.strip()}")
        if got != exp:
            for x in sorted(set(exp) ^ set(got))[:10]:
                print("     diff", x)
        chains = [l.split("\t")[3] for l in out.decode().splitlines() if l.startswith("f\t") and int(l.split("\t")[1]) > 0]
        print(f"     file con catena FAT: {chains.count('catena')}, contigui: {chains.count('contigua')}")
        bad = []
        for p, h in shas.items():
            c, o, e = tool("sha", img, p, bs)
            if c != 0 or o.decode().split()[0] != h:
                bad.append((p, e.strip()))
        check(not bad, f"sha256 identico su {len(shas)} file {bad[:3]}")
        if ranges:
            r = random.Random(42)
            badr = 0
            for p, data in raw.items():
                for _ in range(8):
                    o = r.randint(0, len(data) - 1)
                    n = r.randint(1, 70_000)
                    c, out_, _ = tool("range", img, p, o, n, bs)
                    if out_ != data[o:o + n]:
                        badr += 1
            check(badr == 0, f"letture a caso (posizione e lunghezza qualsiasi) su {len(raw)} file x 8")
        # maiuscole/minuscole indifferenti, anche per le lettere accentate
        if "/Perché è così.txt" in shas:
            c, o, e = tool("sha", img, "/PERCHÉ È COSÌ.TXT", bs)
            check(c == 0 and o.decode().split()[0] == shas["/Perché è così.txt"], "«PERCHÉ È COSÌ.TXT» trova «Perché è così.txt»")
            c, o, e = tool("sha", img, "/non-esiste.txt", bs)
            check(c != 0 and "non trovato" in e, "file inesistente → «file non trovato»")
            c, o, e = tool("sha", img, "/modelli", bs)
            check(c != 0 and "cartella" in e, "una cartella non si legge come file")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--work", default="/tmp/bais-img")
    ap.add_argument("--big", type=float, default=0, help="GB del file grande (0 = salta)")
    args = ap.parse_args()
    os.makedirs(args.work, exist_ok=True)
    sp.run(["cargo", "build", "--release", "--example", "bais-disk"], cwd=ROOT, check=True)
    W = args.work

    # A: volume senza tabella delle partizioni («superfloppy»), settori da 512
    img = f"{W}/a-superfloppy.img"
    run("truncate", "-s", "0", img); run("truncate", "-s", "256M", img)
    with Mounted(img) as m:
        m.mkfs("BAIS-A")
        populate_small(m.mount(), 1)
        m.umount()
        verify(img, None, m, "A superfloppy")
    c, o, _ = tool("info", img)
    check(c == 0 and "BAIS-A" in o.decode() and "maiuscole true" in o.decode(), "etichetta letta, tabella maiuscole verificata: " + o.decode().strip())

    # B: chiavetta GPT come la prepara Windows: ESP FAT32 + dati exFAT, cluster piccoli
    img = f"{W}/b-gpt.img"
    run("truncate", "-s", "0", img); run("truncate", "-s", "512M", img)
    run("sgdisk", "-n1:2048:+64M", "-t1:ef00", "-c1:EFI", "-n2:0:0", "-t2:0700", "-c2:BAIS-DATI", img)
    with Mounted(img, part=2) as m:
        m.mkfs("BAIS-B", ("-c", "4K"))
        mnt = m.mount()
        populate_small(mnt, 2)
        populate_fragmented(mnt, 3)
        m.umount()
        verify(img, 2, m, "B GPT, cluster 4K, file frammentati")
    c, o, _ = tool("parts", img)
    check("efi-system" in o.decode() and "basic-data" in o.decode() and "BAIS-DATI" in o.decode(), "GPT: ESP e partizione dati riconosciute")

    # B2: GPT con l'intestazione principale rovinata → si usa la copia di riserva
    img2 = f"{W}/b2-gpt-rotta.img"
    run("cp", "--sparse=always", img, img2)
    with open(img2, "r+b") as f:
        f.seek(512 + 30); f.write(b"\xff\xff")
    c, o, e = tool("tree", img2)
    check(c == 0 and len(o.decode().splitlines()) > 300, "GPT principale rovinata: letta la copia di riserva")
    with open(img2, "r+b") as f:
        f.seek(0, 2); end = f.tell()
        f.seek(end - 512 + 30); f.write(b"\xff\xff")
    c, o, e = tool("parts", img2)
    check("GPT danneggiata" in o.decode(), "entrambe le GPT rovinate → «GPT danneggiata»")

    # C: MBR con exFAT (tipo 0x07), come le chiavette vendute già formattate
    img = f"{W}/c-mbr.img"
    run("truncate", "-s", "0", img); run("truncate", "-s", "128M", img)
    run("sfdisk", img, input="label: dos\nstart=2048, type=7\n")
    with Mounted(img, part=1) as m:
        m.mkfs("BAIS-C")
        populate_small(m.mount(), 4)
        m.umount()
        verify(img, 1, m, "C MBR", ranges=False)

    # D: disco con settori da 4096 byte (alcuni NVMe/USB): letto con blocchi da 4096 e da 512
    img = f"{W}/d-4k.img"
    run("truncate", "-s", "0", img); run("truncate", "-s", "256M", img)
    with Mounted(img, sector=4096) as m:
        m.mkfs("BAIS-D")
        mnt = m.mount()
        populate_small(mnt, 5)
        populate_fragmented(mnt, 6)
        m.umount()
        verify(img, None, m, "D settori 4K", bs_list=(4096, 512))
    c, o, _ = tool("info", img, 4096)
    check("settore 4096" in o.decode(), "settore del volume 4096 rilevato")

    # E: corruzioni → errori chiari, mai dati sbagliati
    print("[E] corruzioni")
    src = f"{W}/a-superfloppy.img"
    e1 = f"{W}/e1.img"; run("cp", "--sparse=always", src, e1)
    with open(e1, "r+b") as f:
        f.seek(100); b = f.read(1); f.seek(100); f.write(bytes([b[0] ^ 1]))
    c, o, e = tool("tree", e1)
    check(c != 0 and "checksum di avvio" in e, "settore di avvio alterato → checksum di avvio")
    # voce di directory alterata: cambia un byte del nome di un file
    e2 = f"{W}/e2.img"; run("cp", "--sparse=always", src, e2)
    data = open(e2, "rb").read()
    needle = "leggimi".encode("utf-16-le")
    pos = data.find(needle)
    with open(e2, "r+b") as f:
        f.seek(pos); f.write("LEGGIMI".encode("utf-16-le"))
    c, o, e = tool("tree", e2)
    check(c != 0 and "voce di directory" in e, "nome alterato senza aggiornare il checksum → voce non valida")
    # ciclo nella FAT: un file frammentato che punta a sé stesso
    e3 = f"{W}/e3.img"; run("cp", "--sparse=always", f"{W}/b-gpt.img", e3)
    c, o, _ = tool("tree", e3)
    first = [int(l.split("\t")[4]) for l in o.decode().splitlines() if l.split("\t")[2] == "/frammentato-a.bin"][0]
    with open(e3, "r+b") as f:
        part0 = 2048 * 512 + 64 * 1024 * 1024  # inizio della partizione 2 (subito dopo l'ESP)
        f.seek(part0); bs_ = f.read(512)
        fat_off = int.from_bytes(bs_[80:84], "little") * 512
        def fat(c_):
            f.seek(part0 + fat_off + 4 * c_); return int.from_bytes(f.read(4), "little")
        c_ = first
        while True:  # primo salto c_ → v nella catena del file: FAT[v] = c_ chiude un ciclo
            v = fat(c_)
            if v >= 0xFFFFFFF0:
                raise SystemExit("frammentato-a non è frammentato")
            if v != c_ + 1:
                f.seek(part0 + fat_off + 4 * v); f.write(c_.to_bytes(4, "little"))
                break
            c_ = v
    bad = 0
    for p in ("/frammentato-a.bin", "/frammentato-b.bin"):
        c, o, e = tool("sha", e3, p)
        if c != 0 and "catena" in e:
            bad += 1
    check(bad >= 1, "ciclo nella FAT → «catena dei cluster rotta» all'apertura, mai dati sbagliati")

    # F: file grande oltre i 4 GB (come i 17,6 GB di Inkling 2 Mini)
    if args.big > 0:
        gb = args.big
        size = int(gb * (1 << 30)) + 12345
        img = f"{W}/f-grande.img"
        run("truncate", "-s", "0", img); run("truncate", "-s", f"{size + (256 << 20)}", img)
        run("sgdisk", "-n1:2048:0", "-t1:0700", "-c1:BAIS-MODELLI", img)
        with Mounted(img, part=1) as m:
            m.mkfs("MODELLI")
            mnt = m.mount()
            h = hashlib.sha256()
            t = time.time()
            with open(f"{mnt}/inkling-prova.gguf", "wb") as f:
                left = size
                i = 0
                while left:
                    n = min(left, 16 << 20)
                    blk = hashlib.sha256(str(i).encode()).digest() * (n // 32) + b"\0" * (n % 32)
                    f.write(blk); h.update(blk); left -= n; i += 1
            exp = h.hexdigest()
            print(f"[F] scritti {size} byte in {time.time() - t:.0f} s")
            m.umount()
        c, o, e = tool("sha", img, "/inkling-prova.gguf")
        print("    ", o.decode().strip(), e.strip())
        check(c == 0 and o.decode().split()[0] == exp and f"{size} byte" in o.decode(), f"file da {size/1e9:.2f} GB: sha256 identico")
        c, o, e = tool("range", img, "/inkling-prova.gguf", size - 100, 1000)
        check(len(o) == 100, "lettura oltre la fine si ferma a fine file")
        i = (5 << 30) // (16 << 20)
        c, o, e = tool("range", img, "/inkling-prova.gguf", 5 << 30, 64)
        check(o == hashlib.sha256(str(i).encode()).digest() * 2, "lettura a 5 GiB dall'inizio (oltre i 32 bit)")
        os.remove(img)

    print(f"\n{ok_count} verifiche passate, {fail_count} fallite")
    sys.exit(1 if fail_count else 0)


if __name__ == "__main__":
    main()
