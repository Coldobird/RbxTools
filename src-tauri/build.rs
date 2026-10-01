fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=src/exec_in_explorer.cpp");
    let version = std::env::var("CARGO_PKG_VERSION").expect("Cargo package version is unavailable");
    for path in ["tauri.conf.json", "../package.json"] {
        println!("cargo:rerun-if-changed={path}");
        let contents = std::fs::read_to_string(path).expect("Could not read app version file");
        let config: serde_json::Value =
            serde_json::from_str(&contents).expect("App version file is not valid JSON");
        assert_eq!(
            config["version"].as_str(),
            Some(version.as_str()),
            "Version in {path} must match Cargo.toml"
        );
    }

    #[cfg(windows)]
    {
        cc::Build::new()
            .cpp(true)
            .file("src/exec_in_explorer.cpp")
            .compile("exec_in_explorer");
        // Tauri's native dialogs import TaskDialogIndirect from Common Controls
        // v6. GUI diagnostic examples need this dependency too, while retaining
        // the linker's default asInvoker execution level.
        println!("cargo:rustc-link-arg-examples=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-examples=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }

    let manifest = include_str!("app.manifest").replace("RBX_APP_VERSION", &format!("{version}.0"));
    let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("could not build Tauri resources");
}
