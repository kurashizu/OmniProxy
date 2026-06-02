fn main() {
    let mut attrs = tauri_build::Attributes::new();
    // Embed a Windows application manifest that requests administrator
    // privileges. The MSI/NSIS installer will trigger UAC on launch.
    // The dev build (`pnpm tauri dev`) does NOT use this manifest.
    //
    // NOTE: `WindowsAttributes::app_manifest` REPLACES the default manifest
    // (which includes the Common-Controls v6 dependency). We must keep that
    // <dependency> block — otherwise Windows binds the process to the
    // legacy v5 comctl32.dll, which lacks `TaskDialogIndirect` and other
    // symbols Tauri's dialog APIs (and common controls) depend on.
    attrs = attrs.windows_attributes(tauri_build::WindowsAttributes::new().app_manifest(
        r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
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
