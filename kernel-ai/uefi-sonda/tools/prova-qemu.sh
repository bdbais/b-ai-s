#!/bin/sh
# Prova della sonda in QEMU + OVMF, con la chiavetta collegata come USB (xHCI).
#
#   tools/prova-qemu.sh <dimensione file in MiB> [cartella di lavoro]
#
# Crea una chiavetta GPT come quella vera: partizione 1 FAT32 (ESP) con
# EFI/BOOT/BOOTX64.EFI, partizione 2 exFAT con /modelli/inkling-2-mini.gguf
# (contenuto pseudocasuale della dimensione chiesta) e il suo .sha256.
# Serve root (losetup, exfat-fuse), qemu-system-x86_64, ovmf, mtools, gdisk.
set -e
MIB=${1:-256}
W=${2:-/tmp/bais-sonda}
HERE=$(cd "$(dirname "$0")/.." && pwd)
NOME=${NOME:-inkling-2-mini.gguf}   # NOME=altro.gguf prova il ripiego sul .gguf più grande
mkdir -p "$W"
IMG="$W/chiavetta.img"
rm -f "$IMG"
TOTAL=$((MIB + 200))
truncate -s ${TOTAL}M "$IMG"
sgdisk -n1:2048:+64M -t1:ef00 -c1:EFI -n2:0:0 -t2:0700 -c2:BAIS-MODELLI "$IMG" >/dev/null

# Partizione 1: FAT32 con l'applicazione UEFI.
ESP_OFF=$((2048 * 512))
truncate -s 64M "$W/esp.img"
mkfs.vfat -F 32 -n BAIS-EFI "$W/esp.img" >/dev/null
mmd -i "$W/esp.img" ::/EFI ::/EFI/BOOT
mcopy -i "$W/esp.img" "$HERE/dist/BOOTX64.EFI" ::/EFI/BOOT/BOOTX64.EFI
dd if="$W/esp.img" of="$IMG" bs=1M seek=1 conv=notrunc status=none
rm "$W/esp.img"

# Partizione 2: exFAT con il «modello».
P2=$(sfdisk -J "$IMG" | python3 -c 'import json,sys;p=json.load(sys.stdin)["partitiontable"]["partitions"][1];print(p["start"]*512, p["size"]*512)')
set -- $P2
LOOP=$(losetup -f --show -o "$1" --sizelimit "$2" "$IMG")
mkfs.exfat -q -L MODELLI "$LOOP"
mkdir -p "$W/mnt"
mount.exfat-fuse "$LOOP" "$W/mnt"
mkdir -p "$W/mnt/modelli"
python3 - "$W/mnt/modelli/$NOME" "$MIB" <<'EOF'
import hashlib, sys
path, mib = sys.argv[1], int(sys.argv[2])
h = hashlib.sha256()
with open(path, "wb") as f:
    for i in range(mib):
        blk = hashlib.sha256(b"bais%d" % i).digest() * 32768  # 1 MiB
        f.write(blk); h.update(blk)
import os
open(path + ".sha256", "w").write(h.hexdigest() + "  " + os.path.basename(path) + "\n")
open(os.path.join(os.path.dirname(path), "piccolo.gguf"), "wb").write(b"GGUF" + bytes(1000))
print("atteso", h.hexdigest())
EOF
echo "B-AI-S: chiavetta di prova" > "$W/mnt/leggimi.txt"
umount "$W/mnt"
losetup -d "$LOOP"

cp /usr/share/OVMF/OVMF_VARS_4M.fd "$W/vars.fd"
rm -f "$W/seriale.txt"
echo "avvio QEMU (TCG, nessuna accelerazione)..."
START=$(date +%s)
timeout 3600 qemu-system-x86_64 -machine q35 -m 1024 -cpu max -display none -no-reboot \
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
  -drive if=pflash,format=raw,file="$W/vars.fd" \
  -device qemu-xhci,id=xhci \
  -drive if=none,id=chiavetta,format=raw,file="$IMG" \
  -device usb-storage,bus=xhci.0,drive=chiavetta,removable=on,bootindex=0 -nic none \
  -serial file:"$W/seriale.txt" -monitor none || true
echo "QEMU chiuso dopo $(( $(date +%s) - START )) s"
# La console di OVMF manda sequenze ANSI: le togliamo per leggere il risultato.
sed -e 's/\x1b\[[0-9;?]*[a-zA-Z]//g' -e 's/\r//g' "$W/seriale.txt" | grep -v '^\s*$' | sed -n '/B-AI-S sonda/,$p'
