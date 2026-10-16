use std::path::Path;
use tracing::info;

pub struct SparsePackage;

impl SparsePackage {
    /// Generates the Windows 11 sparse package manifest XML for context menu registration.
    pub fn generate_manifest(package_name: &str, publisher_id: &str, exe_path: &str, clsid: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<Package
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  xmlns:desktop4="http://schemas.microsoft.com/appx/manifest/desktop/windows10/4"
  xmlns:desktop10="http://schemas.microsoft.com/appx/manifest/desktop/windows10/10"
  xmlns:com="http://schemas.microsoft.com/appx/manifest/com/windows10"
  IgnorableNamespaces="desktop10">

  <Identity Name="{package_name}" ProcessorArchitecture="neutral" 
    Publisher="{publisher_id}" Version="0.2.4.0"/>  
  <Properties>
    <PublisherDisplayName>DeepNode</PublisherDisplayName>
    <DisplayName>DeepNode Copy Shell Extension</DisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
    <uap10:AllowExternalContent>true</uap10:AllowExternalContent>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily MaxVersionTested="10.0.22631.0" MinVersion="10.0.19041.0" Name="Windows.Desktop"/>
  </Dependencies>
  <Capabilities>
    <rescap:Capability Name="unvirtualizedResources"/>
    <rescap:Capability Name="runFullTrust"/>
  </Capabilities>
  <Applications>
    <Application Id="DeepNode.Copy" Executable="{exe_path}" uap10:RuntimeBehavior="win32App" uap10:TrustLevel="mediumIL">
      <uap:VisualElements DisplayName="DeepNode Copy" Description="Fastest file copy manager" BackgroundColor="transparent" Square150x150Logo="Assets\Square150x150Logo.png" Square44x44Logo="Assets\Square44x44Logo.png"/>
      <Extensions>
        <com:Extension Category="windows.comServer">
          <com:ComServer>
            <com:SurrogateServer DisplayName="DeepNode Copy shell extension">
              <com:Class Id="{clsid}" Path="deepnode_shell.dll" ThreadingModel="STA" />
            </com:SurrogateServer>
          </com:ComServer>
        </com:Extension>
        <desktop4:Extension Category="windows.fileExplorerContextMenus">
          <desktop4:FileExplorerContextMenus>
            <desktop10:ItemType Type="*">
              <desktop10:Verb Clsid="{clsid}" Id="DeepNodeCopy"/>
            </desktop10:ItemType>
            <desktop10:ItemType Type="Directory">
              <desktop10:Verb Clsid="{clsid}" Id="DeepNodeCopy"/>
            </desktop10:ItemType>
            <desktop10:ItemType Type="Directory\Background">
              <desktop10:Verb Clsid="{clsid}" Id="DeepNodeCopy"/>
            </desktop10:ItemType>
          </desktop4:FileExplorerContextMenus>
        </desktop4:Extension>
      </Extensions>
    </Application>
  </Applications>
</Package>"#,
            package_name = package_name,
            publisher_id = publisher_id,
            exe_path = exe_path,
            clsid = clsid
        )
    }

    /// Registers the signed package on Windows 11 using PowerShell Add-AppxPackage cmdlet.
    /// Handles both loose AppxManifest.xml (with -Register) and packaged .msix/.appx (without -Register).
    pub fn register_sparse_package(package_path: &Path) -> std::io::Result<()> {
        info!("Registering Windows 11 package: {:?}", package_path);
        let path_str = package_path.to_string_lossy().to_string();
        let is_manifest = package_path.extension().map(|e| e.eq_ignore_ascii_case("xml")).unwrap_or(false);

        let ps_script = if is_manifest {
            format!(
                "$existing = Get-AppxPackage -Name 'DeepNodeCopy' -ErrorAction SilentlyContinue; \
                 if ($existing) {{ Remove-AppxPackage -Package $existing.PackageFullName -ErrorAction SilentlyContinue }}; \
                 Add-AppxPackage -Path '{}' -Register -ErrorAction Stop",
                path_str
            )
        } else {
            format!(
                "$existing = Get-AppxPackage -Name 'DeepNodeCopy' -ErrorAction SilentlyContinue; \
                 if ($existing) {{ Remove-AppxPackage -Package $existing.PackageFullName -ErrorAction SilentlyContinue }}; \
                 Add-AppxPackage -Path '{}' -ErrorAction Stop",
                path_str
            )
        };

        let status = std::process::Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(ps_script)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "Failed to register Windows 11 package: {:?}",
                package_path
            )))
        }
    }

    /// Unregisters the Windows 11 package using PowerShell Remove-AppxPackage cmdlet.
    pub fn unregister_sparse_package() -> std::io::Result<()> {
        let ps_script = "Get-AppxPackage -Name 'DeepNodeCopy' -ErrorAction SilentlyContinue | Remove-AppxPackage -ErrorAction SilentlyContinue";
        let status = std::process::Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(ps_script)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(
                "Failed to unregister Windows 11 package",
            ))
        }
    }
}
