// Toggle: elevation requires the requireAdministrator manifest (UAC prompt on
// launch) so the low-level keyboard hook can capture while an elevated game
// holds focus (UIPI). Set false for UAC-free local dev/GUI testing; restore to
// true for real use against elevated games.
const ELEVATED: bool = true;

fn main() {
    #[cfg(windows)]
    {
        let level = if ELEVATED { "requireAdministrator" } else { "asInvoker" };
        let manifest = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="{level}" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
</assembly>
"#
        );
        let mut res = winres::WindowsResource::new();
        res.set_manifest(&manifest);
        res.compile().expect("failed to embed manifest");
    }
}
