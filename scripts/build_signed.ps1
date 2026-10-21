# DeepNode Copy - signed release build.
# Signs bin\deepnode_shell.dll, then runs `tauri build`; Tauri signs the exe, installers and
# uninstaller through bundle.windows.signCommand (scripts\sign_file.ps1).
$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")

# Tauri's own SDK lookup can miss installed kits; point it at the newest x64 signtool.
$signtool = Get-Item "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\signtool.exe", "C:\Program Files\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
if (-not $signtool) { throw "signtool.exe bulunamadı! Windows 10/11 SDK kurulu olmalı." }
$env:TAURI_WINDOWS_SIGNTOOL_PATH = $signtool

Push-Location $root
try {
    cargo build -p deepnode-shell --release
    if ($LASTEXITCODE -ne 0) { throw "deepnode-shell derlenemedi" }
    Copy-Item "target\release\deepnode_shell.dll" "bin\deepnode_shell.dll" -Force
    & (Join-Path $PSScriptRoot "sign_file.ps1") (Join-Path $root "bin\deepnode_shell.dll")

    # Build and sign the sparse MSIX before bundling so Tauri ships it as a resource.
    & (Join-Path $PSScriptRoot "build_sparse_package.ps1") -BuildOnly
    if ($LASTEXITCODE -ne 0) { throw "sparse MSIX oluşturulamadı" }

    npx tauri build
    if ($LASTEXITCODE -ne 0) { throw "tauri build başarısız" }

    # Tauri's per-bundle exe patching can leave the portable binary unsigned.
    & (Join-Path $PSScriptRoot "sign_file.ps1") (Join-Path $root "target\release\deepnode-gui.exe")
} finally {
    Pop-Location
}
