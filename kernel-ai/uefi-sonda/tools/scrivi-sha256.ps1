# B-AI-S - scrive accanto a un modello il file <nome>.sha256 che la sonda usa
# per dire VERIFICATO o DIVERSO. Non modifica il modello e non tocca i dischi.
#
#   powershell -ExecutionPolicy Bypass -File scrivi-sha256.ps1 E:\modelli\inkling-2-mini.gguf
#
# Su 17,6 GB serve qualche minuto (dipende dalla chiavetta).
param([Parameter(Mandatory = $true)][string]$Modello)

if (-not (Test-Path -LiteralPath $Modello -PathType Leaf)) {
    Write-Error "File non trovato: $Modello"
    exit 1
}
$inizio = Get-Date
$h = (Get-FileHash -LiteralPath $Modello -Algorithm SHA256).Hash.ToLower()
$nome = Split-Path -Leaf $Modello
# Formato di sha256sum: hash, due spazi, nome. ASCII senza BOM.
[System.IO.File]::WriteAllText("$Modello.sha256", "$h  $nome`n", [System.Text.Encoding]::ASCII)
$s = [int]((Get-Date) - $inizio).TotalSeconds
Write-Host "sha256 $h"
Write-Host "scritto $Modello.sha256 ($s s)"
