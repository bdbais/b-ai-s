#!/usr/bin/env bash
# Collaudo in QEMU (macchina raspi3b: QEMU non emula il Pi 5). Seriale sul terminale.
# uso: tools/qemu-test.sh [modello.gguf]
set -euo pipefail
cd "$(dirname "$0")/.."
DTB=$(mktemp --suffix .dtb)
printf '/dts-v1/;\n/ { model = "QEMU raspi3b"; #address-cells = <1>; #size-cells = <1>; chosen { }; memory@0 { device_type = "memory"; reg = <0 0x3c000000>; }; };\n' | dtc -I dts -O dtb -o "$DTB" -
INITRD=(); [ $# -ge 1 ] && INITRD=(-initrd "$1")
exec qemu-system-aarch64 -M raspi3b -kernel dist/kernel8.img -dtb "$DTB" "${INITRD[@]}" -serial stdio
