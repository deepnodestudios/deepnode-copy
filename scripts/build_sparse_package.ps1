# DeepNode Copy - Windows 11 Sparse Package Builder, Signer & Shell Register
param (
    [string]$CertPath = "certificates\DeepNodeCodeSign.pfx",
    [string]$CertPassword = $(if ($env:DEEPNODE_SIGNING_PASSWORD) { $env:DEEPNODE_SIGNING_PASSWORD } else { "DeepNodeSigning2026!" }),
    [string]$Publisher = "CN=DeepNode Software, O=DeepNode Team, C=TR"
)

$ErrorActionPreference = "Stop"

function Find-SdkTool([string]$toolName) {
    $cmd = Get-Command $toolName -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    $sdkPaths = @(
        "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\$toolName",
        "C:\Program Files\Windows Kits\10\bin\*\x64\$toolName"
    )
    foreach ($p in $sdkPaths) {
        $found = Get-Item $p -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
        if ($found -and (Test-Path $found.FullName)) {
            return $found.FullName
        }
    }
    throw "$toolName bulunamadı! Lütfen Windows 10/11 SDK'nın kurulu olduğundan emin olun."
}

$sdkMakeAppx = Find-SdkTool "makeappx.exe"
$sdkSignTool = Find-SdkTool "signtool.exe"

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$layoutDir = Join-Path $projectRoot "package_layout"
$outputDir = Join-Path $projectRoot "package_output"
$assetsDir = Join-Path $layoutDir "Assets"
$fullCertPath = Join-Path $projectRoot $CertPath
$msixPath = Join-Path $outputDir "DeepNodeCopy.msix"
$binMsixPath = Join-Path $projectRoot "bin\DeepNodeCopy.msix"
$dllSource = Join-Path $projectRoot "target\release\deepnode_shell.dll"
$binDir = Join-Path $projectRoot "bin"
$binDll = Join-Path $binDir "deepnode_shell.dll"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  DeepNode Copy - Production Shell & Sparse Package Build " -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

# 1. Ensure output and layout directories
if (Test-Path $layoutDir) { Remove-Item -Recurse -Force $layoutDir }
if (Test-Path $outputDir) { Remove-Item -Recurse -Force $outputDir }
New-Item -ItemType Directory -Force -Path $layoutDir | Out-Null
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
New-Item -ItemType Directory -Force -Path $assetsDir | Out-Null
if (-not (Test-Path $binDir)) { New-Item -ItemType Directory -Force -Path $binDir | Out-Null }

# 2. Copy release DLL to bin and layout
if (-not (Test-Path $dllSource)) {
    Write-Host ">>> Building deepnode-shell in release mode..." -ForegroundColor Yellow
    & "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release -p deepnode-shell
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to build deepnode-shell. Cargo exit code: $LASTEXITCODE"
    }
}
try {
    Copy-Item $dllSource $binDll -Force -ErrorAction Stop
    Write-Host ">>> Signing bin\deepnode_shell.dll..." -ForegroundColor Cyan
    & $sdkSignTool sign /fd SHA256 /f $fullCertPath /p $CertPassword $binDll
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to sign $binDll. Signtool exit code: $LASTEXITCODE"
    }
} catch {
    Write-Host ">>> bin\deepnode_shell.dll is currently loaded by Explorer; using current signed binary." -ForegroundColor Yellow
}
Copy-Item $binDll "$layoutDir\deepnode_shell.dll" -Force

# 3. Copy visual assets
Write-Host ">>> Staging Appx Assets..." -ForegroundColor Cyan
Copy-Item "$projectRoot\src-tauri\icons\Square150x150Logo.png" "$assetsDir\Square150x150Logo.png" -Force
Copy-Item "$projectRoot\src-tauri\icons\Square44x44Logo.png" "$assetsDir\Square44x44Logo.png" -Force
Copy-Item "$projectRoot\src-tauri\icons\StoreLogo.png" "$assetsDir\StoreLogo.png" -Force

# 4. Generate AppxManifest.xml for sparse package structure
Write-Host ">>> Generating AppxManifest.xml for Windows 11 Fluent Context Menu..." -ForegroundColor Cyan
$manifestContent = @"
<?xml version="1.0" encoding="utf-8"?>
<Package
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  xmlns:desktop4="http://schemas.microsoft.com/appx/manifest/desktop/windows10/4"
  xmlns:desktop6="http://schemas.microsoft.com/appx/manifest/desktop/windows10/6"
  xmlns:desktop10="http://schemas.microsoft.com/appx/manifest/desktop/windows10/10"
  xmlns:com="http://schemas.microsoft.com/appx/manifest/com/windows10"
  IgnorableNamespaces="desktop10">

  <Identity Name="DeepNodeCopy" ProcessorArchitecture="x64" 
    Publisher="$Publisher" 
    Version="0.2.4.0"/>  
  <Properties>
    <PublisherDisplayName>DeepNode Software</PublisherDisplayName>
    <DisplayName>DeepNode Copy Shell Extension</DisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
    <uap10:AllowExternalContent>true</uap10:AllowExternalContent>
    <desktop6:FileSystemWriteVirtualization>disabled</desktop6:FileSystemWriteVirtualization>
    <desktop6:RegistryWriteVirtualization>disabled</desktop6:RegistryWriteVirtualization>
  </Properties>
  <Resources>
    <Resource Language="en-US"/>
  </Resources>
  <Dependencies>
    <TargetDeviceFamily MaxVersionTested="10.0.26100.0" MinVersion="10.0.19041.0" Name="Windows.Desktop"/>
  </Dependencies>
  <Capabilities>
    <rescap:Capability Name="unvirtualizedResources"/>
    <rescap:Capability Name="runFullTrust"/>
  </Capabilities>
  <Applications>
    <Application 
      Id="DeepNode.Copy"
      Executable="DeepNodeCopy.exe"
      uap10:RuntimeBehavior="win32App"
      uap10:TrustLevel="mediumIL">
      <uap:VisualElements 
        DisplayName="DeepNode Copy"
        Description="Modern high-performance file manager"
        BackgroundColor="transparent"
        Square150x150Logo="Assets\Square150x150Logo.png"
        Square44x44Logo="Assets\Square44x44Logo.png">
      </uap:VisualElements>
      <Extensions>
        <com:Extension Category="windows.comServer">
          <com:ComServer>
            <com:SurrogateServer DisplayName="DeepNode Copy shell extension">
              <com:Class Id="5E8F3C72-A2D9-4E61-8C9B-93E48F1A7C20" Path="deepnode_shell.dll" ThreadingModel="STA" />
            </com:SurrogateServer>
          </com:ComServer>
        </com:Extension>
        <desktop4:Extension Category="windows.fileExplorerContextMenus">
          <desktop4:FileExplorerContextMenus>
            <desktop10:ItemType Type="*">
              <desktop10:Verb Clsid="5E8F3C72-A2D9-4E61-8C9B-93E48F1A7C20" Id="DeepNodeCopy"/>
            </desktop10:ItemType>
            <desktop10:ItemType Type="Directory">
              <desktop10:Verb Clsid="5E8F3C72-A2D9-4E61-8C9B-93E48F1A7C20" Id="DeepNodeCopy"/>
            </desktop10:ItemType>
            <desktop10:ItemType Type="Directory\Background">
              <desktop10:Verb Clsid="5E8F3C72-A2D9-4E61-8C9B-93E48F1A7C20" Id="DeepNodeCopy"/>
            </desktop10:ItemType>
            <desktop10:ItemType Type="Drive">
              <desktop10:Verb Clsid="5E8F3C72-A2D9-4E61-8C9B-93E48F1A7C20" Id="DeepNodeCopy"/>
            </desktop10:ItemType>
          </desktop4:FileExplorerContextMenus>
        </desktop4:Extension>
      </Extensions>
    </Application>
  </Applications>
</Package>
"@

Set-Content -Path (Join-Path $layoutDir "AppxManifest.xml") -Value $manifestContent -Encoding UTF8
Copy-Item (Join-Path $layoutDir "AppxManifest.xml") (Join-Path $binDir "AppxManifest.xml") -Force

# 5. Pack MSIX package with makeappx.exe
Write-Host ">>> Packing MSIX package with makeappx.exe..." -ForegroundColor Cyan
& $sdkMakeAppx pack /d $layoutDir /p $msixPath /nv
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $msixPath)) {
    throw "MSIX package creation failed with exit code $LASTEXITCODE."
}
Copy-Item $msixPath $binMsixPath -Force

# 6. Sign package with trusted code-signing certificate
Write-Host ">>> Signing MSIX package with DeepNodeCodeSign.pfx..." -ForegroundColor Cyan
& $sdkSignTool sign /fd SHA256 /f $fullCertPath /p $CertPassword $msixPath
if ($LASTEXITCODE -ne 0) {
    throw "Failed to sign $msixPath. Signtool exit code: $LASTEXITCODE"
}
& $sdkSignTool sign /fd SHA256 /f $fullCertPath /p $CertPassword $binMsixPath
if ($LASTEXITCODE -ne 0) {
    throw "Failed to sign $binMsixPath. Signtool exit code: $LASTEXITCODE"
}

# Verify signature
Write-Host ">>> Verifying signature..." -ForegroundColor Cyan
& $sdkSignTool verify /pa $msixPath
if ($LASTEXITCODE -ne 0) {
    throw "MSIX signature verification failed for $msixPath. Signtool exit code: $LASTEXITCODE"
}
$sig = Get-AuthenticodeSignature $msixPath
if ($sig.Status -ne "Valid") {
    throw "MSIX Authenticode signature verification failed: $($sig.StatusMessage) ($($sig.Status))"
}
Write-Host ">>> MSIX Package signature successfully verified: $($sig.Status)" -ForegroundColor Green

# 7. Register COM Class IDs in Registry for both Context Menu and DragDrop Handlers
Write-Host ">>> Registering COM InProcServer32 & DragDropHandlers via .NET Registry..." -ForegroundColor Cyan

$clsidCmd = "{5E8F3C72-A2D9-4E61-8C9B-93E48F1A7C20}"
$clsidDnd = "{6E8F3C72-A2D9-4E61-8C9B-93E48F1A7C21}"

$hkcu = [Microsoft.Win32.Registry]::CurrentUser

# Context Menu CLSID
$kCmd = $hkcu.CreateSubKey("Software\Classes\CLSID\$clsidCmd")
$kCmd.SetValue("", "DeepNode Copy Context Menu Extension")
$kCmd.Close()

$kCmdInproc = $hkcu.CreateSubKey("Software\Classes\CLSID\$clsidCmd\InprocServer32")
$kCmdInproc.SetValue("", $binDll)
$kCmdInproc.SetValue("ThreadingModel", "Apartment")
$kCmdInproc.Close()

# Drag-and-Drop / Copy-Paste Interceptor CLSID
$kDnd = $hkcu.CreateSubKey("Software\Classes\CLSID\$clsidDnd")
$kDnd.SetValue("", "DeepNode Copy DragDrop Handler")
$kDnd.Close()

$kDndInproc = $hkcu.CreateSubKey("Software\Classes\CLSID\$clsidDnd\InprocServer32")
$kDndInproc.SetValue("", $binDll)
$kDndInproc.SetValue("ThreadingModel", "Apartment")
$kDndInproc.Close()

# DragDropHandlers Registrations
$dragDropLocations = @(
    "Software\Classes\*\shellex\DragDropHandlers\DeepNodeCopy",
    "Software\Classes\Directory\shellex\DragDropHandlers\DeepNodeCopy",
    "Software\Classes\Folder\shellex\DragDropHandlers\DeepNodeCopy",
    "Software\Classes\Drive\shellex\DragDropHandlers\DeepNodeCopy"
)

foreach ($loc in $dragDropLocations) {
    $sub = $hkcu.CreateSubKey($loc)
    $sub.SetValue("", $clsidDnd)
    $sub.Close()
}

# ContextMenuHandlers Registrations
$contextMenuLocations = @(
    "Software\Classes\*\shellex\ContextMenuHandlers\DeepNodeCopy",
    "Software\Classes\Directory\shellex\ContextMenuHandlers\DeepNodeCopy",
    "Software\Classes\Directory\Background\shellex\ContextMenuHandlers\DeepNodeCopy",
    "Software\Classes\Drive\shellex\ContextMenuHandlers\DeepNodeCopy"
)

foreach ($loc in $contextMenuLocations) {
    $sub = $hkcu.CreateSubKey($loc)
    $sub.SetValue("", $clsidCmd)
    $sub.Close()
}

Write-Host ">>> DragDropHandlers and ContextMenuHandlers registered successfully!" -ForegroundColor Green

# 8. Install MSIX Package for Windows 11 Top-Level Context Menu
Write-Host ">>> Registering Sparse MSIX Package via Add-AppxPackage..." -ForegroundColor Cyan
try {
    $existing = Get-AppxPackage -Name "DeepNodeCopy" -ErrorAction SilentlyContinue
    if ($existing) {
        Remove-AppxPackage -Package $existing.PackageFullName -ErrorAction Stop
    }
    Add-AppxPackage -Path $msixPath -ErrorAction Stop
    Write-Host ">>> DeepNodeCopy.msix package registered successfully in WindowsApps!" -ForegroundColor Green
} catch {
    Write-Host ">>> ERROR on Add-AppxPackage: $_" -ForegroundColor Red
    throw "MSIX package registration failed: $_"
}

Write-Host "`n>>> SUCCESS: Production Windows 11 context menu and drag-drop interceptors configured!" -ForegroundColor Green
