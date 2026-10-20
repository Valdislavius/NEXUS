fn main() {
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    // UI, reports, tray and user-scoped autostart always use the caller's token.
    // Privileged sensor providers degrade to explicit permission-denied results.
    let manifest = include_str!("windows-app-manifest.xml");
    let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest.to_owned());
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("Unable to build NEXUS resources");
}
