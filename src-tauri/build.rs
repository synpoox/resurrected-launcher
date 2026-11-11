fn main() {
    // Embed the manifest ourselves for every linked target (tauri-build only covers the
    // bin), so test binaries get Common Controls v6 and asInvoker too.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    // The ACL's list of app commands: anything registered but not listed here is
    // refused at IPC time. Each one also needs `allow-<name>` in capabilities/.
    let commands = tauri_build::AppManifest::new().commands(&[
        "ping",
        "get_product_state",
        "inspect_operation",
        "start_operation",
        "cancel_operation",
        "recover_product",
        "detect_game_locations",
        "choose_game_location",
        "refresh_catalog",
        "play_product",
        "open_loader_download_page",
        "select_game_location",
        "get_release_notes",
        "open_folder",
        "preview_diagnostics",
        "save_diagnostics",
        "show_diagnostics_in_folder",
        "copy_launcher_log",
        "open_logs_folder",
        "get_start_with_windows",
        "set_start_with_windows",
        "minimize_window",
        "close_window",
        "start_window_drag",
        "quit_launcher",
        "get_launcher_update_status",
        "check_for_launcher_update",
        "download_launcher_update",
        "restart_to_update_launcher",
        "open_launcher_whats_new",
        "get_launcher_auto_download",
        "set_launcher_auto_download",
        "get_news",
        "open_news_link",
        "open_product_page",
        "open_community_page",
    ]);
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(windows)
        .app_manifest(commands);
    tauri_build::try_build(attributes).expect("tauri-build failed");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        println!("cargo:rustc-link-arg=/WX");
    }
}
