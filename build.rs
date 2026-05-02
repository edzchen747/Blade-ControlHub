fn main() {
    if std::env::consts::OS == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico"); // Path to your icon
        res.set_manifest_file("app.manifest"); // Keep your admin manifest!
        res.compile().unwrap();
    }

    let mut res = winres::WindowsResource::new();
    res.set_manifest(
        r#"
    <assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
    <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
        <security>
            <requestedPrivileges>
                <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
            </requestedPrivileges>
        </security>
    </trustInfo>
    </assembly>
    "#,
    );
}
