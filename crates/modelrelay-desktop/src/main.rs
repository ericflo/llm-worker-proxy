#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use modelrelay_desktop::{AppSettings, AppStatus};
use tauri::{
    Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};

#[tauri::command]
fn get_status() -> AppStatus {
    AppStatus::default()
}

#[tauri::command]
fn get_settings() -> AppSettings {
    AppSettings::default()
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri commands require owned arguments
fn save_settings(settings: AppSettings) {
    tracing::info!(
        backend_url = %settings.backend_url,
        relay_url = %settings.relay_url,
        "settings saved (stub)"
    );
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_settings,
            save_settings,
        ])
        .setup(|app| {
            let show = MenuItemBuilder::with_id("show", "Open Dashboard").build(app)?;
            let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show)
                .item(&settings)
                .separator()
                .item(&quit)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .tooltip("ModelRelay - Disconnected")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" | "settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
