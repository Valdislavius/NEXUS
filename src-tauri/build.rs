fn main() {
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    // Distributed release builds require UAC. Debug/test harnesses stay asInvoker
    // so Cargo can run tests without elevating the toolchain or third-party tools.
    let manifest = include_str!("windows-app-manifest.xml");
    let manifest = if std::env::var("PROFILE").as_deref() == Ok("release") {
        manifest.to_owned()
    } else {
        manifest.replace("level=\"requireAdministrator\"", "level=\"asInvoker\"")
    };
    let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest);
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("Unable to build NEXUS resources");
}
