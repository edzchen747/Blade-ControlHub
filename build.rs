use std::path::Path;
use std::process::Command;

fn main() {
    build_frontend();
    embed_app_manifest();

    // Tauri owns the icon half of the Windows resource section; the manifest
    // is embedded separately above so that test binaries get it too.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();

    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}

/// Embeds the app manifest into every linked target, not just `[[bin]]`.
///
/// Tauri's window and tray code imports common controls v6 entry points, and
/// the library links it, so a unit-test binary without the manifest binds to
/// comctl32 v5 and dies on startup with STATUS_ENTRYPOINT_NOT_FOUND. Letting
/// tauri-build embed the manifest would cover the executable only, so it is
/// done here for all targets instead.
fn embed_app_manifest() {
    println!("cargo:rerun-if-changed=app.manifest");

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("app.manifest");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}

/// Builds the Svelte UI into `ui/dist`, which Tauri then embeds in the
/// executable. Doing it here keeps `cargo build` the single command that
/// produces a complete binary, rather than a binary with an empty window.
fn build_frontend() {
    for path in [
        "ui/src",
        "ui/index.html",
        "ui/package.json",
        "ui/vite.config.ts",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }

    let ui = Path::new("ui");
    if !ui.join("node_modules").exists() {
        run_npm(
            ui,
            &["install", "--no-audit", "--no-fund"],
            "install UI dependencies",
        );
    }
    run_npm(ui, &["run", "build:fast"], "build the UI");
}

fn run_npm(dir: &Path, args: &[&str], what: &str) {
    // npm is a shell script on Windows, so it is invoked through cmd.
    let status = if cfg!(windows) {
        Command::new("cmd")
            .arg("/C")
            .arg("npm")
            .args(args)
            .current_dir(dir)
            .status()
    } else {
        Command::new("npm").args(args).current_dir(dir).status()
    };

    match status {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("failed to {what}: npm exited with {status}"),
        Err(error) => panic!(
            "failed to {what}: could not run npm ({error}). Node.js is required to build the UI."
        ),
    }
}
