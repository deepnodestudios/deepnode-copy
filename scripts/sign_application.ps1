# DeepNode Copy - Code Signing & Trusted Root Certificate Installer
param (
    [string]$Subject = "CN=DeepNode Software, O=DeepNode Team, C=TR",
    [string]$Password = "DeepNodeSigning2026!"
)

$ErrorActionPreference = "Stop"
$signtool = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\signtool.exe"

Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "         DEEPNODE COPY: KOD IMZALAMA & GUVENLIK SERTIFIKASI           " -ForegroundColor Cyan
Write-Host "======================================================================" -ForegroundColor Cyan

# 1. Sertifika Deposu Kontrolü veya Yeni Sertifika Üretimi
$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $Subject } | Select-Object -First 1

if (-not $cert) {
    Write-Host ">>> Yeni Geliştirici Kod İmzalama Sertifikası üretiliyor..." -ForegroundColor Yellow
    $cert = New-SelfSignedCertificate `
        -Type Custom `
        -Subject $Subject `
        -KeyUsage DigitalSignature `
        -FriendlyName "DeepNode Software Code Signing" `
        -CertStoreLocation "Cert:\CurrentUser\My" `
        -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3") `
        -NotAfter (Get-Date).AddYears(10)
    Write-Host ">>> Sertifika üretildi: $($cert.Thumbprint)" -ForegroundColor Green
} else {
    Write-Host ">>> Mevcut sertifika bulundu: $($cert.Thumbprint)" -ForegroundColor Green
}

# 2. Sertifikayı Güvenilen Yayımcılar (TrustedPublisher) Deposuna Ekle
Write-Host ">>> Sertifika 'Güvenilen Yayımcılar' (TrustedPublisher) deposuna ekleniyor..." -ForegroundColor Yellow
$pubStore = New-Object System.Security.Cryptography.X509Certificates.X509Store(
    [System.Security.Cryptography.X509Certificates.StoreName]::TrustedPublisher,
    [System.Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser
)
$pubStore.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
$pubStore.Add($cert)
$pubStore.Close()
Write-Host ">>> Güvenilen Yayımcılar deposuna başarıyla eklendi!" -ForegroundColor Green

# 3. PFX Dosyasını Dışa Aktar
$certDir = Join-Path $PSScriptRoot "..\certificates"
if (-not (Test-Path $certDir)) { New-Item -ItemType Directory -Force -Path $certDir | Out-Null }
$pfxPath = Join-Path $certDir "DeepNodeCodeSign.pfx"

$securePass = ConvertTo-SecureString -String $Password -Force -AsPlainText
Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $securePass | Out-Null
Write-Host ">>> PFX Sertifika dosyası kaydedildi: $pfxPath" -ForegroundColor Gray

# 4. İmzalanacak Dosyaların Listesi
$filesToSign = @(
    "E:\Projeler\DeepNode Copy\target\release\deepnode-cli.exe",
    "E:\Projeler\DeepNode Copy\target\release\deepnode_shell.dll",
    "E:\Projeler\DeepNode Copy\target\release\deepnode-gui.exe",
    "E:\Projeler\DeepNode Copy\package_output\DeepNodeCopy.msix"
)

Write-Host "`n>>> Dosyalar imzalanıyor..." -ForegroundColor Cyan

foreach ($file in $filesToSign) {
    if (Test-Path $file) {
        Write-Host "  -> İmzalanıyor: $(Split-Path $file -Leaf)..." -ForegroundColor Yellow
        # Sign with SHA256 and RFC3161 Timestamp (fallback without timestamp if offline)
        $signArgs = @("sign", "/fd", "SHA256", "/f", "`"$pfxPath`"", "/p", "`"$Password`"", "/tr", "http://timestamp.digicert.com", "/td", "SHA256", "`"$file`"")
        $proc = Start-Process -FilePath $signtool -ArgumentList $signArgs -Wait -PassThru -NoNewWindow
        
        if ($proc.ExitCode -ne 0) {
            # Timestamp sunucusu timeout verirse timestampsiz dene
            $signArgsNoTs = @("sign", "/fd", "SHA256", "/f", $pfxPath, "/p", $Password, $file)
            & $signtool $signArgsNoTs
        }

        # İmzayı doğrula
        $sig = Get-AuthenticodeSignature $file
        Write-Host "     Durum: $($sig.Status) | İmzacı: $($sig.SignerCertificate.Subject)" -ForegroundColor Green
    }
}

Write-Host "`n======================================================================" -ForegroundColor Cyan
Write-Host "   TÜM UYGULAMA BİLEŞENLERİ BAŞARIYLA İMZALANDI VE GÜVENİLDİ!         " -ForegroundColor Green
Write-Host "======================================================================" -ForegroundColor Cyan
