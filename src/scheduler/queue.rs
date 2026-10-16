use crate::types::DownloadItem;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub name: String,
    pub max_active_downloads: usize,
    pub start_at: Option<chrono::NaiveTime>,
    pub stop_at: Option<chrono::NaiveTime>,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            name: "Main Queue".to_string(),
            max_active_downloads: 1, // default IDM queue runs 1 download at a time
            start_at: None,
            stop_at: None,
        }
    }
}

pub struct QueueManager {
    config: Arc<QueueConfig>,
    pending: Arc<Mutex<VecDeque<DownloadItem>>>,
    active: Arc<Mutex<Vec<DownloadItem>>>,
    completed: Arc<Mutex<Vec<DownloadItem>>>,
    is_running: Arc<AtomicBool>,
}

impl QueueManager {
    pub fn new(config: QueueConfig) -> Self {
        Self {
            config: Arc::new(config),
            pending: Arc::new(Mutex::new(VecDeque::new())),
            active: Arc::new(Mutex::new(Vec::new())),
            completed: Arc::new(Mutex::new(Vec::new())),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn add_item(&self, item: DownloadItem) {
        let mut q = self.pending.lock().await;
        q.push_back(item);
    }

    pub async fn add_and_save(&self, item: DownloadItem, path: &std::path::Path) -> std::io::Result<()> {
        {
            let mut q = self.pending.lock().await;
            q.push_back(item);
        }
        self.save_to_file(path).await
    }

    pub async fn pending_count(&self) -> usize {
        self.pending.lock().await.len()
    }

    pub fn start(&self) {
        self.is_running.store(true, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }

    /// Pulls the next item if concurrent capacity allows
    pub async fn pop_next(&self) -> Option<DownloadItem> {
        if !self.is_running() {
            return None;
        }

        let active_count = self.active.lock().await.len();
        if active_count >= self.config.max_active_downloads {
            return None;
        }

        let mut pending = self.pending.lock().await;
        let item = pending.pop_front()?;
        self.active.lock().await.push(item.clone());
        Some(item)
    }

    pub async fn mark_completed(&self, item: DownloadItem) {
        let mut active = self.active.lock().await;
        active.retain(|x| x.id != item.id);
        self.completed.lock().await.push(item);
    }

    pub async fn list_pending(&self) -> Vec<DownloadItem> {
        self.pending.lock().await.iter().cloned().collect()
    }

    pub async fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let pending = self.pending.lock().await;
        let items: Vec<DownloadItem> = pending.iter().cloned().collect();
        let json = serde_json::to_string_pretty(&items)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)?;
        Ok(())
    }

    pub async fn load_from_file(path: &std::path::Path, config: QueueConfig) -> std::io::Result<Self> {
        let manager = Self::new(config);
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let items: Vec<DownloadItem> = serde_json::from_str(&content)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            let mut q = manager.pending.lock().await;
            for item in items {
                q.push_back(item);
            }
        }
        Ok(manager)
    }
}
