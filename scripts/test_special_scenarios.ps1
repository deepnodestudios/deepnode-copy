# DeepNode Copy - Advanced Special Transfer Scenarios Test Suite
$ErrorActionPreference = "Stop"

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$cliPath = Join-Path $projectRoot "bin\deepnode-cli.exe"
if (-not (Test-Path $cliPath)) {
    $cliPath = Join-Path $projectRoot "target\release\deepnode-cli.exe"
}
$testDir = Join-Path $projectRoot "test_sandbox"
$srcDir = "$testDir\source"
$dstDir = "$testDir\destination"

# Reset sandbox
if (Test-Path $testDir) { Remove-Item -Recurse -Force $testDir }
New-Item -ItemType Directory -Force -Path $srcDir | Out-Null
New-Item -ItemType Directory -Force -Path $dstDir | Out-Null

$failedScenarios = 0

Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "   DEEPNODE COPY: ADVANCED SPECIAL TRANSFER SCENARIOS TEST SUITE      " -ForegroundColor Cyan
Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "CLI binary: $cliPath" -ForegroundColor Gray

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
if ($LASTEXITCODE -ne 0) {
    Write-Host "  -> HATA: CLI copy komutu hata kodu ile döndü: $LASTEXITCODE" -ForegroundColor Red
    $failedScenarios++
} else {
    $dstAdsFile = "$dstDir\document.pdf"
    $stream1 = Get-Content -Path $dstAdsFile -Stream "Zone.Identifier" -ErrorAction SilentlyContinue
    $stream2 = Get-Content -Path $dstAdsFile -Stream "CustomMeta" -ErrorAction SilentlyContinue

    if ($stream1 -match "ZoneId=3" -and $stream2 -match "TopSecret") {
        Write-Host "  -> BASARILI: NTFS ADS Akislari basariyla korundu ve kopyalandi!" -ForegroundColor Green
    } else {
        Write-Host "  -> HATA: ADS Akislari kopyalanamadi!" -ForegroundColor Red
        $failedScenarios++
    }
}

# ---------------------------------------------------------------------------------------------------
# SENARYO 2: SIMD xxHash3 & BÜTÜNLÜK DOĞRULAMA TESTİ
# ---------------------------------------------------------------------------------------------------
Write-Host "`n[SENARYO 2] SIMD xxHash3 Bütünlük Doğrulama Testi..." -ForegroundColor Yellow

$binFile = "$srcDir\database.bin"
$bytes = New-Object byte[] (2 * 1024 * 1024) # 2 MB binary test verisi
(New-Object System.Random).NextBytes($bytes)
[System.IO.File]::WriteAllBytes($binFile, $bytes)

Write-Host "  -> 2 MB rastgele ikili veri üretildi."

# Kaynak dosyanın hash'ini hesapla
$srcVerifyOutput = & $cliPath verify $binFile --hash xxhash64
if ($LASTEXITCODE -ne 0) {
    Write-Host "  -> HATA: Kaynak dosya hash hesaplama hatası!" -ForegroundColor Red
    $failedScenarios++
}
$srcHashMatch = [regex]::Match($srcVerifyOutput, 'Calculated Hash:\s+([a-fA-F0-9]+)')
if (-not $srcHashMatch.Success) {
    Write-Host "  -> HATA: CLI çıktısından hash ayrıştırılamadı!" -ForegroundColor Red
    $failedScenarios++
} else {
    $expectedHash = $srcHashMatch.Groups[1].Value
    Write-Host "  -> Kaynak Hash (xxHash3-64): $expectedHash" -ForegroundColor Gray

    # Transfer et (verify always)
    & $cliPath copy $binFile $dstDir --hash xxhash64 --verify always
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  -> HATA: CLI copy aktarımı başarısız oldu!" -ForegroundColor Red
        $failedScenarios++
    } else {
        # Hedef dosyanın hash'ini beklenen hash ile doğrula
        $dstVerifyOutput = & $cliPath verify "$dstDir\database.bin" --hash xxhash64 --expected $expectedHash
        if ($LASTEXITCODE -eq 0 -and $dstVerifyOutput -match "RESULT: SUCCESS") {
            Write-Host "  -> BASARILI: xxHash3 karma doğrulaması tam olarak eşleşti!" -ForegroundColor Green
        } else {
            Write-Host "  -> HATA: xxHash3 doğrulama eşleşmedi! Çıktı: $dstVerifyOutput" -ForegroundColor Red
            $failedScenarios++
        }
    }
}

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
    Write-Host "  -> DeepNode Copy transferi başlatılıyor (Kilitli dosya atlanmalı/başarısız olmalı, normal dosya başarıyla kopyalanmalı)..."
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
    Write-Host "  -> BASARILI: Kilitli dosya normal dosyanın aktarılmasını engellemedi! normal_data.txt aktarıldı." -ForegroundColor Green
} else {
    Write-Host "  -> HATA: Normal dosya hedefe kopyalanamadı!" -ForegroundColor Red
    $failedScenarios++
}

# ---------------------------------------------------------------------------------------------------
# SENARYO 4: CLI HATA KODU DENETİMİ (NEGATİF TEST)
# ---------------------------------------------------------------------------------------------------
Write-Host "`n[SENARYO 4] CLI Çıkış Kodu (Exit Code) Negatif Testi..." -ForegroundColor Yellow

$procMissing = Start-Process -FilePath $cliPath -ArgumentList @("copy", "tek_arguman") -Wait -PassThru -NoNewWindow
if ($procMissing.ExitCode -eq 2) {
    Write-Host "  -> BASARILI: Eksik argümanda CLI exit code 2 döndü." -ForegroundColor Green
} else {
    Write-Host "  -> HATA: Eksik argümanda exit code 2 bekleniyordu, alınan: $($procMissing.ExitCode)" -ForegroundColor Red
    $failedScenarios++
}

$procVerifyFail = Start-Process -FilePath $cliPath -ArgumentList @("verify", "`"$binFile`"", "--expected", "0000000000000000") -Wait -PassThru -NoNewWindow
if ($procVerifyFail.ExitCode -eq 1) {
    Write-Host "  -> BASARILI: Hash uyuşmazlığında CLI exit code 1 döndü." -ForegroundColor Green
} else {
    Write-Host "  -> HATA: Hash uyuşmazlığında exit code 1 bekleniyordu, alınan: $($procVerifyFail.ExitCode)" -ForegroundColor Red
    $failedScenarios++
}

# ---------------------------------------------------------------------------------------------------
# TEMİZLİK VE SONUÇ
# ---------------------------------------------------------------------------------------------------
if (Test-Path $testDir) { Remove-Item -Recurse -Force $testDir }

if ($failedScenarios -gt 0) {
    Write-Host "`n======================================================================" -ForegroundColor Red
    Write-Host "   TEST BAŞARISIZ: $failedScenarios ADET SENARYO HATA VERDİ!         " -ForegroundColor Red
    Write-Host "======================================================================" -ForegroundColor Red
    exit 1
}

Write-Host "`n======================================================================" -ForegroundColor Cyan
Write-Host "   TÜM ÖZEL TEST SENARYOLARI BAŞARIYLA TAMAMLANDI!                   " -ForegroundColor Green
Write-Host "======================================================================" -ForegroundColor Cyan
exit 0
