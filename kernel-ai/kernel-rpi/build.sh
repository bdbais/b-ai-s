#!/usr/bin/env bash
# Compila kernel-ai per Raspberry e prepara i file per la scheda SD in dist/.
# Requisiti: rustup target add aarch64-unknown-none ; python3
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target aarch64-unknown-none
mkdir -p dist
python3 tools/mkimg.py target/aarch64-unknown-none/release/bais-kernel-rpi dist/kernel_2712.img
cp dist/kernel_2712.img dist/kernel8.img
cp boot/config.txt dist/config.txt
echo "Copia il contenuto di dist/ nella partizione di boot della SD (vedi README.md)."
