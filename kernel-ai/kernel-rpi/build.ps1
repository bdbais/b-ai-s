# Compila kernel-ai per Raspberry e prepara i file per la scheda SD in dist\.
# Requisiti: rustup target add aarch64-unknown-none ; Python 3 (py o python)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
cargo build --release --target aarch64-unknown-none
New-Item -ItemType Directory -Force dist | Out-Null
$py = Get-Command py -ErrorAction SilentlyContinue
if (-not $py) { $py = Get-Command python }
& $py.Source tools\mkimg.py target\aarch64-unknown-none\release\bais-kernel-rpi dist\kernel_2712.img
Copy-Item dist\kernel_2712.img dist\kernel8.img -Force
Copy-Item boot\config.txt dist\config.txt -Force
Write-Host "Copia il contenuto di dist\ nella partizione di boot della SD (vedi README.md)."
