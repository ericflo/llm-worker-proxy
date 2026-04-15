#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use modelrelay_desktop::{AppSettings, AppStatus, WorkerManager};
use tauri::{
    Emitter, Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};

#[tauri::command]
async fn get_has_saved_settings(manager: tauri::State<'_, WorkerManager>) -> Result<bool, String> {
    let settings = manager.get_settings().await;
    // Consider settings "saved" if the user has entered a worker secret
    Ok(!settings.worker_secret.is_empty())
}

#[tauri::command]
async fn get_status(manager: tauri::State<'_, WorkerManager>) -> Result<AppStatus, String> {
    Ok(manager.get_status().await)
}

#[tauri::command]
async fn get_settings(manager: tauri::State<'_, WorkerManager>) -> Result<AppSettings, String> {
    Ok(manager.get_settings().await)
}

#[tauri::command]
async fn save_settings(
    manager: tauri::State<'_, WorkerManager>,
    settings: AppSettings,
) -> Result<(), String> {
    let was_running = manager.is_running().await;
    manager.save_settings(settings).await?;
    if was_running {
        manager.start_worker().await?;
    }
    Ok(())
}

#[tauri::command]
async fn start_worker(manager: tauri::State<'_, WorkerManager>) -> Result<(), String> {
    manager.start_worker().await
}

#[tauri::command]
async fn stop_worker(manager: tauri::State<'_, WorkerManager>) -> Result<(), String> {
    manager.stop_worker().await;
    Ok(())
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
            get_has_saved_settings,
            get_status,
            get_settings,
            save_settings,
            start_worker,
            stop_worker,
        ])
        .setup(|app| {
            // Determine settings file path in the app's data directory
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let settings_path = app_data_dir.join("settings.json");

            let manager = WorkerManager::new(settings_path);

            // If auto_start is enabled, start the worker immediately
            let rt = app.handle().clone();
            let auto_start = {
                let settings_file = app_data_dir.join("settings.json");
                std::fs::read_to_string(settings_file)
                    .ok()
                    .and_then(|json| serde_json::from_str::<AppSettings>(&json).ok())
                    .is_some_and(|s| s.auto_start && !s.worker_secret.is_empty())
            };

            app.manage(manager);

            if auto_start {
                let handle = rt;
                tauri::async_runtime::spawn(async move {
                    let manager = handle.state::<WorkerManager>();
                    if let Err(e) = manager.start_worker().await {
                        tracing::error!(error = %e, "auto-start failed");
                    }
                });
            }

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
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.emit("navigate-tab", "dashboard");
                        }
                    }
                    "settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.emit("navigate-tab", "settings");
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
