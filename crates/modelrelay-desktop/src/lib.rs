use std::path::PathBuf;
use std::sync::Arc;

use modelrelay_worker::{WorkerDaemon, WorkerDaemonConfig};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub mod updater;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub backend_url: String,
    pub relay_url: String,
    pub worker_secret: String,
    pub provider: String,
    pub worker_name: String,
    pub models: Vec<String>,
    pub max_concurrent: u32,
    pub auto_start: bool,
    pub poll_interval_secs: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            backend_url: "http://localhost:11434".to_string(),
            relay_url: "https://api.modelrelay.io".to_string(),
            worker_secret: String::new(),
            provider: "default".to_string(),
            worker_name: "my-worker".to_string(),
            models: vec!["*".to_string()],
            max_concurrent: 4,
            auto_start: false,
            poll_interval_secs: 30,
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AppStatus {
    pub connected: bool,
    pub relay_url: String,
    pub active_requests: u32,
    pub models: Vec<String>,
    pub error: Option<String>,
}

/// Manages the worker daemon lifecycle: start, stop, status, and settings persistence.
pub struct WorkerManager {
    worker_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    settings: Arc<Mutex<AppSettings>>,
    status: Arc<Mutex<AppStatus>>,
    settings_path: PathBuf,
}

impl WorkerManager {
    /// Create a new `WorkerManager`, loading persisted settings from `settings_path` if available.
    #[must_use]
    pub fn new(settings_path: PathBuf) -> Self {
        let settings = match std::fs::read_to_string(&settings_path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => AppSettings::default(),
        };

        Self {
            worker_handle: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(settings)),
            status: Arc::new(Mutex::new(AppStatus::default())),
            settings_path,
        }
    }

    /// Persist current settings to disk.
    fn persist_settings(&self, settings: &AppSettings) -> Result<(), String> {
        if let Some(parent) = self.settings_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        std::fs::write(&self.settings_path, json).map_err(|e| e.to_string())
    }

    pub async fn get_settings(&self) -> AppSettings {
        self.settings.lock().await.clone()
    }

    /// # Errors
    /// Returns an error if settings cannot be persisted to disk.
    pub async fn save_settings(&self, new_settings: AppSettings) -> Result<(), String> {
        self.persist_settings(&new_settings)?;
        *self.settings.lock().await = new_settings;
        Ok(())
    }

    pub async fn get_status(&self) -> AppStatus {
        self.status.lock().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        let handle = self.worker_handle.lock().await;
        handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    /// Start the worker daemon in a background tokio task.
    /// If already running, stops the existing worker first.
    ///
    /// # Errors
    /// Returns an error if the worker secret is empty.
    pub async fn start_worker(&self) -> Result<(), String> {
        self.stop_worker().await;

        let settings = self.settings.lock().await.clone();

        if settings.worker_secret.is_empty() {
            return Err("Worker secret is required".to_string());
        }

        let mut config = WorkerDaemonConfig {
            proxy_base_url: settings.relay_url.clone(),
            provider: settings.provider.clone(),
            worker_secret: settings.worker_secret.clone(),
            worker_name: settings.worker_name.clone(),
            models: settings.models.clone(),
            max_concurrent: settings.max_concurrent,
            backend_base_url: settings.backend_url.clone(),
            endpoint_prefixes: vec![],
        };
        config.resolve_wildcard_models().await;

        let status = Arc::clone(&self.status);
        let worker_handle = Arc::clone(&self.worker_handle);

        // Update status to reflect we're starting
        {
            let mut s = status.lock().await;
            s.connected = true;
            s.relay_url.clone_from(&settings.relay_url);
            s.models.clone_from(&config.models);
            s.error = None;
        }

        let handle = tokio::spawn(async move {
            let daemon = WorkerDaemon::new(config);
            let result = daemon.run_with_reconnect().await;

            // Worker exited — update status
            let mut s = status.lock().await;
            s.connected = false;
            s.active_requests = 0;
            if let Err(e) = result {
                s.error = Some(e.to_string());
            }

            // Clear the handle reference
            *worker_handle.lock().await = None;
        });

        *self.worker_handle.lock().await = Some(handle);
        Ok(())
    }

    /// Stop the running worker daemon.
    pub async fn stop_worker(&self) {
        let mut handle = self.worker_handle.lock().await;
        if let Some(h) = handle.take() {
            h.abort();
            let _ = h.await;
        }
        drop(handle);

        let mut s = self.status.lock().await;
        s.connected = false;
        s.active_requests = 0;
        s.error = None;
    }
}
