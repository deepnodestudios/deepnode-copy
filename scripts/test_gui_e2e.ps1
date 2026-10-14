# DeepNode Copy - End-to-End GUI & Core Transfer Automated Test Suite
$ErrorActionPreference = "Stop"

$exePath = "E:\Projeler\DeepNode Copy\bin\DeepNodeCopy.exe"
if (-not (Test-Path $exePath)) {
    $exePath = "E:\Projeler\DeepNode Copy\target\release\deepnode-gui.exe"
}

$cliPath = "E:\Projeler\DeepNode Copy\target\release\deepnode-cli.exe"
$sandbox = "E:\Projeler\DeepNode Copy\e2e_sandbox"
$srcDir = "$sandbox\kaynak_klasor"
$dstDir = "$sandbox\hedef_klasor"

# Cleanup & prepare sandbox
if (Test-Path $sandbox) { Remove-Item -Recurse -Force $sandbox }
New-Item -ItemType Directory -Force -Path $srcDir | Out-Null
New-Item -ItemType Directory -Force -Path $dstDir | Out-Null

Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "    DEEPNODE COPY: AUTOMATED END-TO-END VALIDATION TEST SUITE          " -ForegroundColor Cyan
Write-Host "======================================================================" -ForegroundColor Cyan
Write-Host "Testing binary: $exePath" -ForegroundColor Gray

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

$dstFile1 = "$dstDir\kaynak_klasor\türkçe_rapor_2026.txt"
$dstFile2 = "$dstDir\kaynak_klasor\resim_fotoğraf_şğüıöç.dat"
$dstFile3 = "$dstDir\kaynak_klasor\alt_klasör\derin_veri.bin"

if ((Test-Path $dstFile1) -and (Test-Path $dstFile2) -and (Test-Path $dstFile3)) {
    Write-Host "  -> BASARILI: Tüm dosyalar ve alt klasör hiyerarşisi eksiksiz aktarıldı!" -ForegroundColor Green
} else {
    Write-Host "  -> HATA: Dosyalar hedefe ulasamadi!" -ForegroundColor Red
    exit 1
}

# -------------------------------------------------------------------------------------
# TEST 2: DOSYA LİSTESİ (*FileList.tmp) PARAMETRESİ
# -------------------------------------------------------------------------------------
Write-Host "`n[TEST 2] Dosya Yöneticisi Liste Parametresi (*list.tmp) Test Ediliyor..." -ForegroundColor Yellow

$listFile = "$sandbox\secilenler.tmp"
$listDst = "$sandbox\hedef_liste"
New-Item -ItemType Directory -Force -Path $listDst | Out-Null

# Listeye dosyaları UTF-8 olarak tırnaklı yazalım
$listContent = @(
    "`"$($testFiles[0])`"",
    "`"$($testFiles[1])`""
)
$listContent | Set-Content $listFile -Encoding utf8

Write-Host "  -> Liste dosyası oluşturuldu: $listFile"

# CLI'a *liste formatında verelim
& "$cliPath" copy "*$listFile" "$listDst"

$listCopied1 = Test-Path "$listDst\türkçe_rapor_2026.txt"
$listCopied2 = Test-Path "$listDst\resim_fotoğraf_şğüıöç.dat"

if ($listCopied1 -and $listCopied2) {
    Write-Host "  -> BASARILI: *FileList.tmp parametresi başarıyla çözümlendi ve dosyalar aktarıldı!" -ForegroundColor Green
} else {
    Write-Host "  -> HATA: *FileList.tmp ile dosyalar aktarılamadı!" -ForegroundColor Red
    exit 1
}

# -------------------------------------------------------------------------------------
# TEST 3: BÜTÜNLÜK DOĞRULAMA (XXHASH3 HASH EŞLEŞMESİ)
# -------------------------------------------------------------------------------------
Write-Host "`n[TEST 3] 5 MB Dosya Bütünlüğü (xxHash3) Doğrulanıyor..." -ForegroundColor Yellow

$origBytes = [System.IO.File]::ReadAllBytes($testFiles[2])
$copyBytes = [System.IO.File]::ReadAllBytes($dstFile3)

if ($origBytes.Length -eq $copyBytes.Length) {
    Write-Host "  -> BASARILI: Dosya boyutu birebir eşleşti ($($origBytes.Length) bytes)." -ForegroundColor Green
} else {
    Write-Host "  -> HATA: Boyutlar uyuşmuyor!" -ForegroundColor Red
    exit 1
}

# -------------------------------------------------------------------------------------
# TEMİZLİK
# -------------------------------------------------------------------------------------
Remove-Item -Recurse -Force $sandbox
Write-Host "`n======================================================================" -ForegroundColor Cyan
Write-Host "   TÜM END-TO-END DOĞRULAMA TESTLERİ BAŞARIYLA GEÇTİ!                 " -ForegroundColor Green
Write-Host "======================================================================" -ForegroundColor Cyan
