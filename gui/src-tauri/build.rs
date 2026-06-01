fn main() {
    let mut attrs = tauri_build::Attributes::new();
    // Embed a Windows application manifest that requests administrator
    // privileges. The MSI/NSIS installer will trigger UAC on launch.
    // The dev build (`pnpm tauri dev`) does NOT use this manifest.
    attrs = attrs.windows_attributes(tauri_build::WindowsAttributes::new().app_manifest(
        r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#,
    ));
    tauri_build::try_build(attrs).expect("failed to run tauri build script");
}
