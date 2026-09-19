fn main() {
    #[cfg(windows)]
    cc::Build::new()
        .cpp(true)
        .file("src/exec_in_explorer.cpp")
        .compile("exec_in_explorer");

    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("could not build Tauri resources");
}
