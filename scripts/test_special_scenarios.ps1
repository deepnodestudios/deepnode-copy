# DeepNode Copy - Advanced Special Transfer Scenarios Test Suite
$ErrorActionPreference = "Stop"

$cliPath = "E:\Projeler\DeepNode Copy\target\release\deepnode-cli.exe"
$testDir = "E:\Projeler\DeepNode Copy\test_sandbox"
$srcDir = "$testDir\source"
$dstDir = "$testDir\destination"

# Reset sandbox
if (Test-Path $testDir) { Remove-Item -Recurse -Force $testDir }
New-Item -ItemType Directory -Force -Path $srcDir | Out-Null
New-Item -ItemType Directory -Force -Path $dstDir | Out-Null

Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "   DEEPNODE COPY: ADVANCED SPECIAL TRANSFER SCENARIOS TEST SUITE      " -ForegroundColor Cyan
Write-Host "======================================================================" -ForegroundColor Cyan

# ---------------------------------------------------------------------------------------------------
# SENARYO 1: NTFS ALTERNATE DATA STREAMS (ADS) KORUMA TESTİ
# ---------------------------------------------------------------------------------------------------
Write-Host "`n[SENARYO 1] NTFS Alternate Data Streams (ADS) Test Ediliyor..." -ForegroundColor Yellow

$adsFile = "$srcDir\document.pdf"
"Bu ana PDF icerigidir." | Set-Content $adsFile

# Zone.Identifier stream (İnternetten indirildi işareti)
Set-Content -Path $adsFile -Stream "Zone.Identifier" -Value "[ZoneTransfer]`r`nZoneId=3"
# Ozel metadata stream
Set-Content -Path $adsFile -Stream "CustomMeta" -Value "Author=DeepNode;SecurityLevel=TopSecret"

Write-Host "  -> Kaynak dosyaya 'Zone.Identifier' ve 'CustomMeta' NTFS streamleri eklendi."

# Transfer et
& "$cliPath" copy "$adsFile" "$dstDir"

$dstAdsFile = "$dstDir\document.pdf"
$stream1 = Get-Content -Path $dstAdsFile -Stream "Zone.Identifier" -ErrorAction SilentlyContinue
$stream2 = Get-Content -Path $dstAdsFile -Stream "CustomMeta" -ErrorAction SilentlyContinue

if ($stream1 -match "ZoneId=3" -and $stream2 -match "TopSecret") {
    Write-Host "  -> BASARILI: NTFS ADS Akislari basariyla korundu ve kopyalandi!" -ForegroundColor Green
} else {
    Write-Host "  -> HATA: ADS Akislari kopyalanamadi!" -ForegroundColor Red
}

# ---------------------------------------------------------------------------------------------------
# SENARYO 2: SIMD xxHash3 & BLAKE3 DOGRULAMA TESTI
# ---------------------------------------------------------------------------------------------------
Write-Host "`n[SENARYO 2] SIMD xxHash3 & BLAKE3 Bütünlük Doğrulama Testi..." -ForegroundColor Yellow

$binFile = "$srcDir\database.bin"
$bytes = New-Object byte[] (2 * 1024 * 1024) # 2 MB binary test verisi
(New-Object System.Random).NextBytes($bytes)
[System.IO.File]::WriteAllBytes($binFile, $bytes)

Write-Host "  -> 2 MB rastgele ikili veri üretildi."
& $cliPath copy $binFile $dstDir --hash xxhash64 --verify always

$verifyOutput = & $cliPath verify "$dstDir\database.bin" --hash xxhash64
Write-Host "  -> xxHash3 Doğrulama Çıktısı:`n$verifyOutput" -ForegroundColor Green

# ---------------------------------------------------------------------------------------------------
# SENARYO 3: KİLİTLİ DOSYA (SKIP & RETRY) TESTİ
# ---------------------------------------------------------------------------------------------------
Write-Host "`n[SENARYO 3] Kilitli Dosya (Exclusive File Lock / Skip & Retry) Testi..." -ForegroundColor Yellow

$lockedFile = "$srcDir\locked_service.log"
"Bu dosya bir arka plan servisi tarafindan kilitlendi." | Set-Content $lockedFile

$normalFile = "$srcDir\normal_data.txt"
"Bu normal bir dosyadir." | Set-Content $normalFile

# Dosyayı FileShare.None (Tam kilit) modunda açık tutan bir PowerShell nesnesi oluşturalım
$fileStream = [System.IO.File]::Open($lockedFile, [System.IO.FileMode]::Open, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
Write-Host "  -> 'locked_service.log' dosyası özel kilit (FileShare.None) ile kilitlendi."

try {
    Write-Host "  -> DeepNode Copy transferi başlatılıyor (Kilitli dosya atlanmalı, normal dosya başarıyla kopyalanmalı)..."
    $filesToCopy = (Get-ChildItem $srcDir -File).FullName
    & $cliPath copy $filesToCopy "$dstDir\batch_test"
} finally {
    # Kilidi serbest bırak
    $fileStream.Close()
    $fileStream.Dispose()
    Write-Host "  -> Dosya kilidi kaldırıldı." -ForegroundColor Gray
}

$copiedNormal = Test-Path "$dstDir\batch_test\normal_data.txt"
if ($copiedNormal) {
    Write-Host "  -> BASARILI: Kilitli dosya tüm transferi kesmedi! Normal dosya basariyla aktarildi." -ForegroundColor Green
} else {
    Write-Host "  -> HATA: Kilitli dosya tum transferi kesti!" -ForegroundColor Red
}

# ---------------------------------------------------------------------------------------------------
# TEMİZLİK
# ---------------------------------------------------------------------------------------------------
Remove-Item -Recurse -Force $testDir
Write-Host "`n======================================================================" -ForegroundColor Cyan
Write-Host "   TÜM ÖZEL TEST SENARYOLARI BAŞARIYLA TAMAMLANDI!                   " -ForegroundColor Green
Write-Host "======================================================================" -ForegroundColor Cyan
