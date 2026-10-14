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
    Publisher="{publisher_id}" Version="1.0.0.0"/>  
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

    /// Registers the sparse signed package on Windows 11 using PowerShell Add-AppxPackage cmdlet.
    pub fn register_sparse_package(msix_path: &Path) -> std::io::Result<()> {
        info!("Registering Windows 11 sparse package: {:?}", msix_path);
        let status = std::process::Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(format!("Add-AppxPackage -Path '{}' -Register", msix_path.display()))
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to register sparse package",
            ))
        }
    }
}
