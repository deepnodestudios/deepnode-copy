# DeepNode Copy - End-to-End GUI & Core Transfer Automated Test Suite
$ErrorActionPreference = "Stop"

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$exePath = Join-Path $projectRoot "bin\DeepNodeCopy.exe"
if (-not (Test-Path $exePath)) {
    $exePath = Join-Path $projectRoot "target\release\deepnode-gui.exe"
}

$cliPath = Join-Path $projectRoot "bin\deepnode-cli.exe"
if (-not (Test-Path $cliPath)) {
    $cliPath = Join-Path $projectRoot "target\release\deepnode-cli.exe"
}

$sandbox = Join-Path $projectRoot "e2e_sandbox"
$srcDir = "$sandbox\kaynak_klasor"
$dstDir = "$sandbox\hedef_klasor"

# Cleanup & prepare sandbox
if (Test-Path $sandbox) { Remove-Item -Recurse -Force $sandbox }
New-Item -ItemType Directory -Force -Path $srcDir | Out-Null
New-Item -ItemType Directory -Force -Path $dstDir | Out-Null

$failedTests = 0

Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "    DEEPNODE COPY: AUTOMATED END-TO-END VALIDATION TEST SUITE          " -ForegroundColor Cyan
Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "Testing GUI binary: $exePath" -ForegroundColor Gray
Write-Host "Testing CLI binary: $cliPath" -ForegroundColor Gray

# -------------------------------------------------------------------------------------
# TEST 1: ÇOKLU DOSYA VE TÜRKÇE KARAKTER AKTARIMI
# -------------------------------------------------------------------------------------
Write-Host "`n[TEST 1] Türkçe karakterli çoklu dosya ve klasör kopyalama..." -ForegroundColor Yellow

$testFiles = @(
    "$srcDir\türkçe_rapor_2026.txt",
    "$srcDir\resim_fotoğraf_şğüıöç.dat",
    "$srcDir\alt_klasör\derin_veri.bin"
)

New-Item -ItemType Directory -Force -Path "$srcDir\alt_klasör" | Out-Null
"Bu Türkçe bir test belgesidir: ŞŞşşĞĞğğÜÜüüİıÖöÇç" | Set-Content $testFiles[0] -Encoding utf8
"Binary dosya verisi" | Set-Content $testFiles[1] -Encoding utf8

$dummyBytes = New-Object byte[] (5 * 1024 * 1024) # 5 MB
(New-Object System.Random).NextBytes($dummyBytes)
[System.IO.File]::WriteAllBytes($testFiles[2], $dummyBytes)

Write-Host "  -> 3 adet test dosyası oluşturuldu (1 tanesi 5 MB ikili veri)."

# CLI ile aktarım testi
& "$cliPath" copy "$srcDir" "$dstDir" --hash xxhash64 --verify always
if ($LASTEXITCODE -ne 0) {
    Write-Host "  -> HATA: CLI copy aktarımı hata kodu ile döndü: $LASTEXITCODE" -ForegroundColor Red
    $failedTests++
} else {
    $dstFile1 = "$dstDir\kaynak_klasor\türkçe_rapor_2026.txt"
    $dstFile2 = "$dstDir\kaynak_klasor\resim_fotoğraf_şğüıöç.dat"
    $dstFile3 = "$dstDir\kaynak_klasor\alt_klasör\derin_veri.bin"

    if ((Test-Path $dstFile1) -and (Test-Path $dstFile2) -and (Test-Path $dstFile3)) {
        Write-Host "  -> BASARILI: Tüm dosyalar ve alt klasör hiyerarşisi eksiksiz aktarıldı!" -ForegroundColor Green
    } else {
        Write-Host "  -> HATA: Dosyalar hedefe ulasamadi!" -ForegroundColor Red
        $failedTests++
    }
}

# -------------------------------------------------------------------------------------
# TEST 2: DOSYA LİSTESİ (*FileList.tmp) PARAMETRESİ
# -------------------------------------------------------------------------------------
Write-Host "`n[TEST 2] Dosya Yöneticisi Liste Parametresi (*list.tmp) Test Ediliyor..." -ForegroundColor Yellow

$listFile = "$sandbox\secilenler.tmp"
$listDst = "$sandbox\hedef_liste"
New-Item -ItemType Directory -Force -Path $listDst | Out-Null

$listContent = @(
    "$($testFiles[0])",
    "$($testFiles[1])"
)
$listContent | Set-Content $listFile -Encoding utf8

Write-Host "  -> Liste dosyası oluşturuldu: $listFile"

# CLI'a *liste formatında verelim
& "$cliPath" copy "*$listFile" "$listDst"
if ($LASTEXITCODE -ne 0) {
    Write-Host "  -> HATA: *FileList.tmp ile kopyalama başarısız oldu!" -ForegroundColor Red
    $failedTests++
} else {
    $listCopied1 = Test-Path "$listDst\türkçe_rapor_2026.txt"
    $listCopied2 = Test-Path "$listDst\resim_fotoğraf_şğüıöç.dat"

    if ($listCopied1 -and $listCopied2) {
        Write-Host "  -> BASARILI: *FileList.tmp parametresi başarıyla çözümlendi ve dosyalar aktarıldı!" -ForegroundColor Green
    } else {
        Write-Host "  -> HATA: *FileList.tmp ile dosyalar aktarılamadı!" -ForegroundColor Red
        $failedTests++
    }
}

# -------------------------------------------------------------------------------------
# TEST 3: GERÇEK KRİPTOGRAFİK BÜTÜNLÜK DOĞRULAMA (SIMD xxHash3)
# -------------------------------------------------------------------------------------
Write-Host "`n[TEST 3] 5 MB Dosya Bütünlüğü (Gerçek SIMD xxHash3 Hash Doğrulaması)..." -ForegroundColor Yellow

# Dosya boyut kontrolü
$origBytes = [System.IO.File]::ReadAllBytes($testFiles[2])
$copyBytes = [System.IO.File]::ReadAllBytes($dstFile3)

if ($origBytes.Length -ne $copyBytes.Length) {
    Write-Host "  -> HATA: Dosya boyutları uyuşmuyor!" -ForegroundColor Red
    $failedTests++
} else {
    # Gerçek xxHash3-64 sağlama hesaplama ve karşılıklı doğrulama
    $srcHashOut = & $cliPath verify $testFiles[2] --hash xxhash64
    $srcHashMatch = [regex]::Match($srcHashOut, 'Calculated Hash:\s+([a-fA-F0-9]+)')

    if (-not $srcHashMatch.Success) {
        Write-Host "  -> HATA: Kaynak dosyanın xxHash3 karması hesaplanamadı!" -ForegroundColor Red
        $failedTests++
    } else {
        $expectedHash = $srcHashMatch.Groups[1].Value
        Write-Host "  -> Hesaplanan xxHash3-64 Karması: $expectedHash" -ForegroundColor Gray

        # Hedef dosyayı bu hash ile doğrula
        $verifyOut = & $cliPath verify $dstFile3 --hash xxhash64 --expected $expectedHash
        if ($LASTEXITCODE -eq 0 -and $verifyOut -match "RESULT: SUCCESS") {
            Write-Host "  -> BASARILI: 5 MB ikili dosyanın xxHash3 karması kusursuz eşleşti ($expectedHash)!" -ForegroundColor Green
        } else {
            Write-Host "  -> HATA: xxHash3 karma doğrulaması başarısız!" -ForegroundColor Red
            $failedTests++
        }
    }
}

# -------------------------------------------------------------------------------------
# TEST 4: GERÇEK GUI (DeepNodeCopy.exe) E2E AKTARIM VE OTOMATİK KAPANIŞ
# -------------------------------------------------------------------------------------
Write-Host "`n[TEST 4] Gerçek GUI (DeepNodeCopy.exe) E2E Aktarım ve /close Kapanış Testi..." -ForegroundColor Yellow

$guiSrc = "$sandbox\gui_kaynak"
$guiDst = "$sandbox\gui_hedef"
New-Item -ItemType Directory -Force -Path $guiSrc | Out-Null
New-Item -ItemType Directory -Force -Path $guiDst | Out-Null

$guiSample = "$guiSrc\gui_e2e_payload.bin"
$guiData = New-Object byte[] (1024 * 1024) # 1 MB
(New-Object System.Random).NextBytes($guiData)
[System.IO.File]::WriteAllBytes($guiSample, $guiData)

Write-Host "  -> GUI başlatılıyor: $exePath copy `"$guiSrc`" `"$guiDst`" /close"
$guiProc = $null
for ($attempt = 1; $attempt -le 3; $attempt++) {
    try {
        $guiProc = Start-Process -FilePath $exePath -ArgumentList @("copy", "`"$guiSrc`"", "`"$guiDst`"", "/close") -Wait -PassThru -ErrorAction Stop
        break
    } catch {
        if ($attempt -lt 3) {
            Start-Sleep -Milliseconds 1000
        } else {
            throw $_
        }
    }
}

$copiedGuiFile = "$guiDst\gui_kaynak\gui_e2e_payload.bin"
if ($guiProc.ExitCode -eq 0 -and (Test-Path $copiedGuiFile)) {
    # Hash doğrulaması
    $guiHashOut = & $cliPath verify "$guiSample" --hash xxhash64 | Out-String
    $guiHashMatch = [regex]::Match($guiHashOut, 'Calculated Hash:\s+([a-fA-F0-9]+)')

    if ($guiHashMatch.Success) {
        $guiHash = $guiHashMatch.Groups[1].Value
        $guiVerifyOut = & $cliPath verify "$copiedGuiFile" --hash xxhash64 --expected $guiHash | Out-String
        if ($LASTEXITCODE -eq 0 -and $guiVerifyOut -match "RESULT: SUCCESS") {
            Write-Host "  -> BASARILI: GUI arka planda aktarımı tamamladı, xxHash3 doğrulandı ve /close ile temiz kapandı (ExitCode: 0)!" -ForegroundColor Green
        } else {
            Write-Host "  -> HATA: GUI tarafından aktarılan dosyanın sağlama doğrulaması başarısız!" -ForegroundColor Red
            $failedTests++
        }
    } else {
        Write-Host "  -> HATA: Kaynak dosya hash çıktısı alınamadı! Çıktı: $guiHashOut" -ForegroundColor Red
        $failedTests++
    }
} else {
    Write-Host "  -> HATA: GUI süreci başarısız oldu (ExitCode: $($guiProc.ExitCode)) veya dosya aktarılamadı!" -ForegroundColor Red
    $failedTests++
}

# -------------------------------------------------------------------------------------
# TEMİZLİK VE SONUÇ
# -------------------------------------------------------------------------------------
if (Test-Path $sandbox) { Remove-Item -Recurse -Force $sandbox }

if ($failedTests -gt 0) {
    Write-Host "`n======================================================================" -ForegroundColor Red
    Write-Host "   E2E TEST BAŞARISIZ: $failedTests ADET TEST HATA VERDİ!            " -ForegroundColor Red
    Write-Host "======================================================================" -ForegroundColor Red
    exit 1
}

Write-Host "`n======================================================================" -ForegroundColor Cyan
Write-Host "   TÜM END-TO-END DOĞRULAMA TESTLERİ BAŞARIYLA GEÇTİ!                 " -ForegroundColor Green
Write-Host "======================================================================" -ForegroundColor Cyan
exit 0
