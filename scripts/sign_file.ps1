# DeepNode Copy - signs a single file with the "CN=DeepNode Studios" certificate used by earlier releases.
# Used by Tauri's bundle.windows.signCommand (main exe, installer, uninstaller, NSIS plugins) and for
# deepnode_shell.dll before bundling. The certificate is taken from the CurrentUser\My store by thumbprint.
param (
    [Parameter(Mandatory = $true)][string]$File,
    [string]$Thumbprint = $(if ($env:DEEPNODE_SIGNING_THUMBPRINT) { $env:DEEPNODE_SIGNING_THUMBPRINT } else { "CD49095665273FCDBD7B0C34052F110B8A4F748E" })
)

$ErrorActionPreference = "Stop"

$signtool = $env:TAURI_WINDOWS_SIGNTOOL_PATH
if (-not $signtool) { $signtool = (Get-Command signtool.exe -ErrorAction SilentlyContinue).Source }
if (-not $signtool) {
    $signtool = Get-Item "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe", "C:\Program Files\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $signtool) { throw "signtool.exe bulunamadı! Windows 10/11 SDK kurulu olmalı." }

$cert = Get-Item "Cert:\CurrentUser\My\$Thumbprint" -ErrorAction SilentlyContinue
if (-not $cert -or -not $cert.HasPrivateKey) { throw "İmza sertifikası (private key ile) bulunamadı: CurrentUser\My\$Thumbprint" }

# RFC3161 timestamp keeps the signature valid after the certificate expires; retry without it when offline.
& $signtool sign /fd SHA256 /sha1 $Thumbprint /s My /tr http://timestamp.digicert.com /td SHA256 $File | Out-Host
if ($LASTEXITCODE -ne 0) {
    & $signtool sign /fd SHA256 /sha1 $Thumbprint /s My $File | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "İmzalama başarısız: $File" }
}

$sig = Get-AuthenticodeSignature $File
Write-Host "Signed $(Split-Path $File -Leaf): $($sig.Status) ($($sig.SignerCertificate.Subject))"
