#!/usr/bin/env bash
# Compila kernel-ai v0.3-pc e crea l'ISO avviabile (GRUB, BIOS) in dist/.
# Requisiti: rust stabile, grub-pc-bin, xorriso, mtools; python3 + gguf + numpy (+ autograd per bais-intent).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --config cargo-config.toml
mkdir -p dist iso/boot/grub
cp target/release/bais-kernel-pc iso/boot/bais-kernel.elf
cp boot/grub.cfg iso/boot/grub/grub.cfg
if [ ! -f dist/bais-model.gguf ]; then
    python3 ../tests/gen_llama.py dist/bais-model.gguf q8_0
fi
cp dist/bais-model.gguf iso/boot/bais-model.gguf
if [ ! -f dist/bais-intent.gguf ]; then
    python3 ../tests/gen_intent.py dist/bais-intent.gguf   # classificatore degli intenti (numpy + autograd + gguf)
fi
cp dist/bais-intent.gguf iso/boot/bais-intent.gguf
grub-mkrescue -o dist/bais-kernel-v0.3-pc.iso iso -- -volid BAIS_V03 2>/dev/null
echo "ISO pronta: dist/bais-kernel-v0.3-pc.iso"
