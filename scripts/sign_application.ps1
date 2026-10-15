# DeepNode Copy - Code Signing & Trusted Root Certificate Installer
param (
    [string]$Subject = "CN=DeepNode Software, O=DeepNode Team, C=TR",
    [string]$Password = $(if ($env:DEEPNODE_SIGNING_PASSWORD) { $env:DEEPNODE_SIGNING_PASSWORD } else { "DeepNodeSigning2026!" })
)

$ErrorActionPreference = "Stop"

function Find-SignTool {
    $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    $sdkPaths = @(
        "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe",
        "C:\Program Files\Windows Kits\10\bin\*\x64\signtool.exe"
    )
    foreach ($p in $sdkPaths) {
        $found = Get-Item $p -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
        if ($found -and (Test-Path $found.FullName)) {
            return $found.FullName
        }
    }
    throw "signtool.exe bulunamadı! Lütfen Windows 10/11 SDK'nın kurulu olduğundan emin olun."
}
$signtool = Find-SignTool

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

# 2. Sertifikayı Güvenilen Yayımcılar (TrustedPublisher) ve Güvenilen Kişiler (TrustedPeople) Deposuna Ekle
Write-Host ">>> Sertifika 'Güvenilen Yayımcılar' ve 'Güvenilen Kişiler' deposuna ekleniyor..." -ForegroundColor Yellow
$pubStore = New-Object System.Security.Cryptography.X509Certificates.X509Store(
    [System.Security.Cryptography.X509Certificates.StoreName]::TrustedPublisher,
    [System.Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser
)
$pubStore.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
$pubStore.Add($cert)
$pubStore.Close()

$peopleStore = New-Object System.Security.Cryptography.X509Certificates.X509Store(
    [System.Security.Cryptography.X509Certificates.StoreName]::TrustedPeople,
    [System.Security.Cryptography.X509Certificates.StoreLocation]::CurrentUser
)
$peopleStore.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite)
$peopleStore.Add($cert)
$peopleStore.Close()
Write-Host ">>> Güvenlik depolarına başarıyla eklendi!" -ForegroundColor Green

# 3. PFX Dosyasını Dışa Aktar
$certDir = Join-Path $PSScriptRoot "..\certificates"
if (-not (Test-Path $certDir)) { New-Item -ItemType Directory -Force -Path $certDir | Out-Null }
$pfxPath = Join-Path $certDir "DeepNodeCodeSign.pfx"

$securePass = ConvertTo-SecureString -String $Password -Force -AsPlainText
Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $securePass | Out-Null
Write-Host ">>> PFX Sertifika dosyası kaydedildi: $pfxPath" -ForegroundColor Gray

# 4. İmzalanacak Dosyaların Listesi
$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$filesToSign = @(
    (Join-Path $projectRoot "target\release\deepnode-cli.exe"),
    (Join-Path $projectRoot "target\release\deepnode_shell.dll"),
    (Join-Path $projectRoot "target\release\deepnode-gui.exe"),
    (Join-Path $projectRoot "bin\deepnode-cli.exe"),
    (Join-Path $projectRoot "bin\deepnode_shell.dll"),
    (Join-Path $projectRoot "bin\DeepNodeCopy.exe"),
    (Join-Path $projectRoot "package_output\DeepNodeCopy.msix"),
    (Join-Path $projectRoot "bin\DeepNodeCopy.msix")
)

Write-Host "`n>>> Dosyalar imzalanıyor..." -ForegroundColor Cyan

$signedCount = 0
foreach ($file in $filesToSign) {
    if (Test-Path $file) {
        Write-Host "  -> İmzalanıyor: $(Split-Path $file -Leaf)..." -ForegroundColor Yellow
        # Sign with SHA256 and RFC3161 Timestamp (fallback without timestamp if offline)
        $signArgs = @("sign", "/fd", "SHA256", "/f", "`"$pfxPath`"", "/p", "`"$Password`"", "/tr", "http://timestamp.digicert.com", "/td", "SHA256", "`"$file`"")
        $proc = Start-Process -FilePath $signtool -ArgumentList $signArgs -Wait -PassThru -NoNewWindow
        
        if ($proc.ExitCode -ne 0) {
            # Timestamp sunucusu timeout verirse timestampsiz dene
            $signArgsNoTs = @("sign", "/fd", "SHA256", "/f", $pfxPath, "/p", $Password, $file)
            $procNoTs = Start-Process -FilePath $signtool -ArgumentList $signArgsNoTs -Wait -PassThru -NoNewWindow
            if ($procNoTs.ExitCode -ne 0) {
                throw "Signtool signing failed for $file with exit code $($procNoTs.ExitCode)"
            }
        }

        # İmzayı doğrula
        $sig = Get-AuthenticodeSignature $file
        if ($sig.Status -ne "Valid") {
            throw "Signature verification failed for $file. Status: $($sig.StatusMessage) ($($sig.Status))"
        }
        Write-Host "     Durum: $($sig.Status) | İmzacı: $($sig.SignerCertificate.Subject)" -ForegroundColor Green
        $signedCount++
    }
}

if ($signedCount -eq 0) {
    throw "Hiçbir imzalanacak dosya bulunamadı! Lütfen önce derlemeyi tamamlayın."
}

Write-Host "`n======================================================================" -ForegroundColor Cyan
Write-Host "   TOPLAM $signedCount ADET BİLEŞEN BAŞARIYLA İMZALANDI VE DOĞRULANDI! " -ForegroundColor Green
Write-Host "======================================================================" -ForegroundColor Cyan
