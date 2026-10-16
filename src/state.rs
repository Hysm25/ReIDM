use crate::types::DownloadItem;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct StateManager {
    base_dir: PathBuf,
}

impl StateManager {
    pub fn new(base_dir: PathBuf) -> Self {
        fs::create_dir_all(&base_dir).ok();
        Self { base_dir }
    }

    /// Generates a unique task directory name: {filename}_{id}
    pub fn get_task_dir(&self, filename: &str, id: u64) -> PathBuf {
        let clean_name = Path::new(filename)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        self.base_dir.join(format!("{}_{}", clean_name, id))
    }

    /// Saves the download item metadata as JSON and writes the binary chunk record table
    pub fn save_state(&self, item: &DownloadItem) -> std::io::Result<()> {
        fs::create_dir_all(&item.temp_dir)?;

        // 1. Write metadata JSON
        let meta_file = item.temp_dir.join("metadata.json");
        let json = serde_json::to_string_pretty(item)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(meta_file, json)?;

        // 2. Write 16-byte ChunkRecords (IDM ChList binary representation)
        let chlist_file = item.temp_dir.join("chunks.bin");
        let mut ch_file = File::create(chlist_file)?;
        for seg in &item.segments {
            let record = seg.to_chunk_record();
            ch_file.write_all(&record.to_bytes())?;
        }

        // 3. Append to log file
        self.log_event(
            &item.temp_dir,
            &format!(
                "State saved. Status: {:?}, Downloaded: {} / {} bytes across {} segments",
                item.status,
                item.downloaded,
                item.file_size,
                item.segments.len()
            ),
        )?;

        Ok(())
    }

    /// Loads a saved download item from its task directory
    pub fn load_state(&self, task_dir: &Path) -> std::io::Result<DownloadItem> {
        let meta_file = task_dir.join("metadata.json");
        let content = fs::read_to_string(meta_file)?;
        let mut item: DownloadItem = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Reconcile actual segment file sizes on disk
        for seg in &mut item.segments {
            seg.is_active = false;
            if seg.temp_file.exists() {
                let actual_len = fs::metadata(&seg.temp_file)?.len();
                seg.downloaded = actual_len;
                seg.current_pos = (seg.start_pos + actual_len).min(seg.end_pos + 1);
            } else {
                seg.downloaded = 0;
                seg.current_pos = seg.start_pos;
            }
        }

        item.downloaded = item.segments.iter().map(|s| s.downloaded).sum();
        Ok(item)
    }

    /// Finds a paused download task directory for a given URL if one exists
    pub fn find_resumable_task(&self, url: &str) -> Option<PathBuf> {
        let entries = fs::read_dir(&self.base_dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let meta_file = path.join("metadata.json");
                if meta_file.exists() {
                    if let Ok(content) = fs::read_to_string(&meta_file) {
                        if let Ok(item) = serde_json::from_str::<DownloadItem>(&content) {
                            if item.url == url && item.status == crate::types::DownloadStatus::Paused {
                                return Some(path);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Lists all paused or interrupted tasks in the temp directory
    pub fn list_paused_tasks(&self) -> Vec<DownloadItem> {
        let mut list = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Ok(item) = self.load_state(&path) {
                        if item.status == crate::types::DownloadStatus::Paused {
                            list.push(item);
                        }
                    }
                }
            }
        }
        list
    }

    /// Logs an event to the task log file ({filename}_{id}.log), matching IDM's logging behavior
    pub fn log_event(&self, task_dir: &Path, msg: &str) -> std::io::Result<()> {
        let log_file = task_dir.join("task.log");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;

        let now = chrono::Local::now();
        writeln!(
            file,
            "[{}] {}",
            now.format("%Y-%m-%d %H:%M:%S%.3f"),
            msg
        )?;
        Ok(())
    }

    /// Cleans up temporary task files and directory after successful assembly
    pub fn cleanup_task(&self, task_dir: &Path) -> std::io::Result<()> {
        if task_dir.exists() {
            fs::remove_dir_all(task_dir)?;
        }
        Ok(())
    }
}
