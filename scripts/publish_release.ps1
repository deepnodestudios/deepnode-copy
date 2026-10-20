# DeepNode Copy - packages the signed build artifacts under versioned names and optionally
# publishes them to GitHub. Artifacts must come from scripts\build_signed.ps1 (signed build).
param(
    [string]$Version = "",
    [switch]$Upload,
    [switch]$Draft,
    [switch]$AllowUnsigned
)
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")

if (-not $Version) {
    $Version = (Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).version
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Geçersiz sürüm: '$Version'" }
$tag = "v$Version"

$pkgVersion = (Get-Content (Join-Path $root "package.json") -Raw | ConvertFrom-Json).version
$cargoVersion = (Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
if ($pkgVersion -ne $Version -or $cargoVersion -ne $Version) {
    throw "Sürüm uyuşmuyor: package.json=$pkgVersion, Cargo.toml=$cargoVersion, tauri.conf.json=$Version"
}

$sources = @(
    @{ Path = (Join-Path $root "target\release\bundle\nsis\DeepNode Copy_${Version}_x64-setup.exe"); Name = "DeepNodeCopy_Setup_x64_$tag.exe" },
    @{ Path = (Join-Path $root "target\release\bundle\msi\DeepNode Copy_${Version}_x64_en-US.msi");  Name = "DeepNodeCopy_Setup_x64_$tag.msi" },
    @{ Path = (Join-Path $root "target\release\deepnode-gui.exe");                                   Name = "deepnode-gui_$tag.exe" }
)
foreach ($s in $sources) {
    if (-not (Test-Path $s.Path)) { throw "Derleme çıktısı bulunamadı: $($s.Path). Önce scripts\build_signed.ps1 çalıştırın." }
}

$outDir = Join-Path $root "release_assets\$tag"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

foreach ($s in $sources) {
    $dest = Join-Path $outDir $s.Name
    Copy-Item -Force $s.Path $dest
    $sig = Get-AuthenticodeSignature $dest
    Write-Host ("{0}  ({1:N0} bayt, imza: {2})" -f $s.Name, (Get-Item $dest).Length, $sig.Status)
    if ($sig.Status -ne "Valid" -and -not $AllowUnsigned) {
        throw "İmza doğrulanamadı: $($s.Name) ($($sig.Status)). -AllowUnsigned ile atlanabilir."
    }
}
Write-Host "Paket hazır: $outDir"

if (-not $Upload) {
    Write-Host "Yayınlamak için: scripts\publish_release.ps1 -Version $Version -Upload"
    return
}

git -C $root rev-parse -q --verify "refs/tags/$tag" *> $null
if ($LASTEXITCODE -ne 0) { throw "'$tag' etiketi yok. Önce sürümü commit'leyip 'git tag $tag' ve 'git push origin $tag' çalıştırın." }

gh release view $tag *> $null
if ($LASTEXITCODE -ne 0) {
    $notesFile = Join-Path $root "release_notes_$tag.md"
    $createArgs = @("release", "create", $tag, "--title", "DeepNode Copy $tag", "--verify-tag")
    if (Test-Path $notesFile) { $createArgs += @("--notes-file", $notesFile) } else { $createArgs += "--generate-notes" }
    if ($Draft) { $createArgs += "--draft" }
    gh @createArgs
    if ($LASTEXITCODE -ne 0) { throw "Release oluşturulamadı: $tag" }
}

$uploadArgs = @("release", "upload", $tag, "--clobber") + ($sources | ForEach-Object { Join-Path $outDir $_.Name })
gh @uploadArgs
if ($LASTEXITCODE -ne 0) { throw "Varlıklar yüklenemedi: $tag" }
Write-Host "Yayınlandı: https://github.com/deepnodestudios/deepnode-copy/releases/tag/$tag"
