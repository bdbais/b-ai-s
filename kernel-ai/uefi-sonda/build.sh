#!/bin/sh
# Costruisce BOOTX64.EFI: C (gnu-efi) + bais-storage in Rust no_std.
# Serve: gcc, binutils, gnu-efi (apt install gnu-efi), Rust stabile per x86_64-unknown-linux-gnu.
set -e
cd "$(dirname "$0")"
EFIINC=/usr/include/efi
EFILIB=/usr/lib
# Rust: codice indipendente dalla posizione e senza red zone, come vuole UEFI.
RUSTFLAGS="-C relocation-model=pic -C no-redzone=yes -C code-model=small" \
  cargo build --release --manifest-path ffi/Cargo.toml
gcc -c main.c -o build-main.o -O2 -fno-stack-protector -fpic -fshort-wchar -mno-red-zone \
  -ffreestanding -fno-builtin -DEFI_FUNCTION_WRAPPER -I$EFIINC -I$EFIINC/x86_64 -Wall
ld -nostdlib -znocombreloc -shared -Bsymbolic --no-undefined -T bais_efi.lds \
  $EFILIB/crt0-efi-x86_64.o build-main.o ffi/target/release/libbais_sonda_ffi.a \
  -L$EFILIB -lefi -lgnuefi -o build-sonda.so
mkdir -p dist
objcopy -j .text -j .sdata -j .data -j .dynamic -j .dynsym -j .rel -j .rela -j .rel.* -j .rela.* -j .reloc \
  --target efi-app-x86_64 --subsystem=10 build-sonda.so dist/BOOTX64.EFI
ls -l dist/BOOTX64.EFI
