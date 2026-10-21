# DeepNode Copy - signs build outputs with the same "CN=DeepNode Studios" certificate as
# DeepNode Download Manager (DDM). The certificate lives in CurrentUser\My; no PFX is used.
param (
    [string]$Thumbprint = $(if ($env:DEEPNODE_SIGNING_THUMBPRINT) { $env:DEEPNODE_SIGNING_THUMBPRINT } else { "548A57C15C1AAAACAC7EDE40AE73042D42295EC9" })
)

$ErrorActionPreference = "Stop"
$signScript = Join-Path $PSScriptRoot "sign_file.ps1"

Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "         DEEPNODE COPY: KOD IMZALAMA & GUVENLIK SERTIFIKASI           " -ForegroundColor Cyan
Write-Host "======================================================================" -ForegroundColor Cyan

$cert = Get-Item "Cert:\CurrentUser\My\$Thumbprint" -ErrorAction SilentlyContinue
if (-not $cert -or -not $cert.HasPrivateKey) {
    throw "İmza sertifikası (private key ile) bulunamadı: CurrentUser\My\$Thumbprint. DDM ile aynı sertifikanın kurulu olduğundan emin olun."
}
Write-Host ">>> Sertifika bulundu: $($cert.Thumbprint) ($($cert.Subject))" -ForegroundColor Green

# 1. Public sertifikayı güvenilen depolara ekle (MSIX/TrustedPeople doğrulaması için)
Write-Host ">>> Sertifika 'Güvenilen Yayımcılar' ve 'Güvenilen Kişiler' deposuna ekleniyor..." -ForegroundColor Yellow
foreach ($storeName in @("TrustedPublisher", "TrustedPeople")) {
    $store = New-Object System.Security.Cryptography.X509Certificates.X509Store(
        [System.Security.Cryptography.X509Certificates.StoreName]::$storeName,
        [System.Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser
    )
    $store.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
    $store.Add($cert)
    $store.Close()
}
Write-Host ">>> Güvenlik depolarına başarıyla eklendi!" -ForegroundColor Green

# 2. İmzalanacak Dosyaların Listesi
$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$filesToSign = @(
    (Join-Path $projectRoot "target\release\deepnode-cli.exe"),
    (Join-Path $projectRoot "target\release\deepnode_shell.dll"),
    (Join-Path $projectRoot "target\release\deepnode-gui.exe"),
    (Join-Path $projectRoot "bin\deepnode-cli.exe"),
    (Join-Path $projectRoot "bin\deepnode_shell.dll"),
    (Join-Path $projectRoot "package_output\DeepNodeCopy.msix"),
    (Join-Path $projectRoot "bin\DeepNodeCopy.msix")
)

# Also sign any installer bundles found in target\release\bundle
$bundleFiles = Get-ChildItem (Join-Path $projectRoot "target\release\bundle") -Recurse -Include "*.exe", "*.msi" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName
if ($bundleFiles) {
    $filesToSign += $bundleFiles
}

Write-Host "`n>>> Dosyalar imzalanıyor..." -ForegroundColor Cyan

$signedCount = 0
foreach ($file in $filesToSign) {
    if (Test-Path $file) {
        Write-Host "  -> İmzalanıyor: $(Split-Path $file -Leaf)..." -ForegroundColor Yellow
        & $signScript -File $file -Thumbprint $Thumbprint
        if ($LASTEXITCODE -ne 0) {
            throw "İmzalama başarısız: $file"
        }
        $signedCount++
    }
}

if ($signedCount -eq 0) {
    throw "Hiçbir imzalanacak dosya bulunamadı! Lütfen önce derlemeyi tamamlayın."
}

Write-Host "`n======================================================================" -ForegroundColor Cyan
Write-Host "   TOPLAM $signedCount ADET BİLEŞEN BAŞARIYLA İMZALANDI VE DOĞRULANDI! " -ForegroundColor Green
Write-Host "======================================================================" -ForegroundColor Cyan
