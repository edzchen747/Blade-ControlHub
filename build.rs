fn main() {
    if std::env::consts::OS == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico"); // Path to your icon

        if std::env::var("PROFILE").unwrap_or_default() != "debug" {
            res.set_manifest_file("app.manifest");
        }

        res.compile().unwrap();
    }
}
