use serde::{Deserialize, Serialize};

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
}
