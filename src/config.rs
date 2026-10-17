use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub const DEFAULT_MAX_CONNECTIONS: usize = 8;
#[allow(dead_code)]
pub const MIN_CONNECTIONS: usize = 1;
#[allow(dead_code)]
pub const MAX_CONNECTIONS: usize = 32;

pub const DEFAULT_MIN_SPLIT_SIZE: u64 = 256 * 1024; // 256 KB
pub const DEFAULT_BUFFER_SIZE: usize = 64 * 1024; // 64 KB read buffer
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_WS_PORT: u16 = 1001;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub download_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub state_dir: PathBuf,
    pub max_connections: usize,
    pub min_split_size: u64,
    pub buffer_size: usize,
    pub timeout_secs: u64,
    pub speed_limit: Option<u64>, // bytes/sec, None for unlimited
    pub user_agent: String,
    pub ws_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));

        let download_dir = home.join("Downloads");
        let base_data = home.join(".local").join("share").join("reidm");
        let temp_dir = base_data.join("temp");
        let state_dir = base_data.join("state");

        Self {
            download_dir,
            temp_dir,
            state_dir,
            max_connections: DEFAULT_MAX_CONNECTIONS,
            min_split_size: DEFAULT_MIN_SPLIT_SIZE,
            buffer_size: DEFAULT_BUFFER_SIZE,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            speed_limit: None,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            ws_port: DEFAULT_WS_PORT,
        }
    }
}

impl Config {
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.download_dir)?;
        std::fs::create_dir_all(&self.temp_dir)?;
        std::fs::create_dir_all(&self.state_dir)?;
        Ok(())
    }
}
