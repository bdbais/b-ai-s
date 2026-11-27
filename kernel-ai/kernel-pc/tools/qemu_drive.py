#!/usr/bin/env python3
"""Avvia l'ISO in QEMU (senza finestra) e scrive frasi sulla seriale COM1, una alla volta.

uso: qemu_drive.py <iso> <log> "frase 1" "frase 2" ...
Aspetta che il kernel sia pronto, invia ogni frase seguita da invio, attende che l'uscita
si fermi e alla fine salva tutta la seriale in <log>. Opzionale: SCREEN=<file.ppm> salva
una schermata alla fine.
"""
import os
import socket
import subprocess
import sys
import time

iso, log, lines = sys.argv[1], sys.argv[2], sys.argv[3:]
sock = "/tmp/bais-serial.sock"
mon = "/tmp/bais-mon.sock"
for p in (sock, mon):
    if os.path.exists(p):
        os.unlink(p)
q = subprocess.Popen([
    "qemu-system-x86_64", "-m", os.environ.get("MEM", "512"), "-cpu", os.environ.get("CPU", "qemu64"), "-cdrom", iso, "-display", "none",
    "-chardev", f"socket,id=s0,path={sock},server=on,wait=on", "-serial", "chardev:s0",
    "-monitor", f"unix:{mon},server=on,wait=off", "-no-reboot",
], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
for _ in range(100):
    if os.path.exists(sock):
        break
    time.sleep(0.05)
s = socket.socket(socket.AF_UNIX)
s.connect(sock)
s.settimeout(0.2)
buf = bytearray()


def pump(quiet_s, max_s):
    """Legge finché la seriale resta muta per quiet_s secondi (o max_s in tutto)."""
    t0 = last = time.time()
    while time.time() - t0 < max_s:
        try:
            d = s.recv(65536)
            if not d:
                break
            buf.extend(d)
            last = time.time()
        except socket.timeout:
            if time.time() - last > quiet_s and time.time() - t0 > 0.5:
                break


pump(4.0, 120)
for ln in lines:
    buf.extend(f"\n>>> {ln}\n".encode())
    for ch in (ln + "\r").encode():
        s.send(bytes([ch]))
        time.sleep(0.01)
    pump(float(os.environ.get("QUIET", "3")), float(os.environ.get("MAXS", "300")))
if os.environ.get("SCREEN"):
    m = socket.socket(socket.AF_UNIX)
    m.connect(mon)
    m.send(f"screendump {os.environ['SCREEN']}\n".encode())
    time.sleep(1.5)
    m.close()
q.kill()
open(log, "wb").write(bytes(buf))
print(bytes(buf).decode("utf-8", "replace"))
