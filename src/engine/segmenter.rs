use crate::config::Config;
use crate::engine::assembler::Assembler;
use crate::engine::connection::ConnectionManager;
use crate::engine::rate_limiter::RateLimiter;
use crate::state::StateManager;
use crate::types::{DownloadItem, DownloadStatus, Segment};
use futures_util::StreamExt;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub download_id: u64,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub speed_bps: u64,
    pub active_connections: usize,
    pub segments: Vec<SegmentProgress>,
    pub status: DownloadStatus,
}

#[derive(Debug, Clone)]
pub struct SegmentProgress {
    pub id: u32,
    pub start_pos: u64,
    pub current_pos: u64,
    pub end_pos: u64,
    pub downloaded: u64,
    pub is_active: bool,
}

pub struct DynamicSegmenter {
    config: Arc<Config>,
    conn_manager: Arc<ConnectionManager>,
    state_manager: Arc<StateManager>,
    rate_limiter: Arc<RateLimiter>,
    progress_tx: broadcast::Sender<ProgressUpdate>,
}

impl DynamicSegmenter {
    pub fn new(
        config: Arc<Config>,
        conn_manager: Arc<ConnectionManager>,
        state_manager: Arc<StateManager>,
        rate_limiter: Arc<RateLimiter>,
    ) -> (Self, broadcast::Receiver<ProgressUpdate>) {
        let (progress_tx, progress_rx) = broadcast::channel(128);
        (
            Self {
                config,
                conn_manager,
                state_manager,
                rate_limiter,
                progress_tx,
            },
            progress_rx,
        )
    }

    /// Executes the download using dynamic in-half segmentation
    pub async fn download(
        &self,
        item: &mut DownloadItem,
        stop_signal: Arc<AtomicBool>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        item.status = DownloadStatus::Downloading;
        self.state_manager.save_state(item)?;

        // If file is not resumable or single-stream is required (file_size unknown or 1 connection)
        if !item.resumable || item.file_size == 0 || self.config.max_connections <= 1 {
            return self.download_single_stream(item, stop_signal).await;
        }

        self.download_multi_stream(item, stop_signal).await
    }

    /// Multi-connection dynamic segmentation algorithm matching IDM
    async fn download_multi_stream(
        &self,
        item: &mut DownloadItem,
        stop_signal: Arc<AtomicBool>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Initialize segments if empty
        if item.segments.is_empty() {
            let chunk0_file = Self::get_chunk_filepath(&item.temp_dir, &item.file_name, 0);
            let seg0 = Segment::new(
                0,
                0,
                item.file_size.saturating_sub(1),
                -1,
                chunk0_file,
            );
            item.segments.push(seg0);
        }

        let shared_segments = Arc::new(Mutex::new(item.segments.clone()));
        let next_chunk_id = Arc::new(AtomicU32::new(
            shared_segments
                .lock()
                .await
                .iter()
                .map(|s| s.id)
                .max()
                .unwrap_or(0)
                + 1,
        ));

        let active_workers = Arc::new(AtomicU32::new(0));
        let total_downloaded = Arc::new(AtomicU64::new(item.downloaded));
        let max_conns = self.config.max_connections;

        // Progress monitor loop
        let progress_tx = self.progress_tx.clone();
        let mon_segments = shared_segments.clone();
        let mon_downloaded = total_downloaded.clone();
        let mon_active = active_workers.clone();
        let mon_stop = stop_signal.clone();
        let download_id = item.id;
        let total_bytes = item.file_size;

        let monitor_handle = tokio::spawn(async move {
            let mut last_bytes = mon_downloaded.load(Ordering::Relaxed);
            let mut last_time = Instant::now();

            while !mon_stop.load(Ordering::Relaxed) {
                sleep(Duration::from_millis(500)).await;
                let now = Instant::now();
                let current_bytes = mon_downloaded.load(Ordering::Relaxed);
                let elapsed = now.duration_since(last_time).as_secs_f64();
                let speed = if elapsed > 0.0 && current_bytes >= last_bytes {
                    ((current_bytes - last_bytes) as f64 / elapsed) as u64
                } else {
                    0
                };
                last_bytes = current_bytes;
                last_time = now;

                let segs = mon_segments.lock().await;
                let seg_progress: Vec<SegmentProgress> = segs
                    .iter()
                    .map(|s| SegmentProgress {
                        id: s.id,
                        start_pos: s.start_pos,
                        current_pos: s.current_pos,
                        end_pos: s.end_pos,
                        downloaded: s.downloaded,
                        is_active: s.is_active,
                    })
                    .collect();

                let _ = progress_tx.send(ProgressUpdate {
                    download_id,
                    total_bytes,
                    downloaded_bytes: current_bytes,
                    speed_bps: speed,
                    active_connections: mon_active.load(Ordering::Relaxed) as usize,
                    segments: seg_progress,
                    status: DownloadStatus::Downloading,
                });
            }
        });

        // Worker orchestration loop: keep up to max_conns active workers downloading segments
        let mut worker_handles = Vec::new();

        while !stop_signal.load(Ordering::Relaxed) {
            let current_active = active_workers.load(Ordering::Relaxed) as usize;

            // Check if all segments are completed
            let all_finished = {
                let segs = shared_segments.lock().await;
                segs.iter().all(|s| s.is_finished())
            };

            if all_finished {
                break;
            }

            if current_active < max_conns {
                // Find work to assign: either an unfinished inactive segment, or split the largest active segment
                let work_to_start = {
                    let mut segs = shared_segments.lock().await;

                    // 1. Look for an existing unfinished, inactive segment
                    let mut found_existing = None;
                    for seg in segs.iter_mut() {
                        if !seg.is_finished() && !seg.is_active {
                            seg.is_active = true;
                            found_existing = Some(seg.id);
                            break;
                        }
                    }

                    if let Some(seg_id) = found_existing {
                        Some(seg_id)
                    } else {
                        // 2. IDM in-half division rule: find the segment with the largest remaining bytes
                        let mut best_idx = None;
                        let mut max_remaining = 0;

                        for (idx, seg) in segs.iter().enumerate() {
                            let rem = seg.remaining();
                            if rem > max_remaining {
                                max_remaining = rem;
                                best_idx = Some(idx);
                            }
                        }

                        if let Some(idx) = best_idx {
                            if max_remaining >= self.config.min_split_size * 2 {
                                let new_id = next_chunk_id.fetch_add(1, Ordering::Relaxed);
                                let new_temp = Self::get_chunk_filepath(
                                    &item.temp_dir,
                                    &item.file_name,
                                    new_id,
                                );

                                if let Some(mut new_seg) =
                                    segs[idx].split_half(new_id, new_temp)
                                {
                                    new_seg.is_active = true;
                                    let new_seg_id = new_seg.id;
                                    segs.push(new_seg);

                                    let _ = self.state_manager.log_event(
                                        &item.temp_dir,
                                        &format!(
                                            "Insert next chunk in list: split chunk {} -> new chunk {}",
                                            segs[idx].id, new_seg_id
                                        ),
                                    );

                                    Some(new_seg_id)
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                };

                if let Some(seg_id) = work_to_start {
                    active_workers.fetch_add(1, Ordering::Relaxed);

                    let segs_ref = shared_segments.clone();
                    let active_ref = active_workers.clone();
                    let total_dl_ref = total_downloaded.clone();
                    let conn_mgr = self.conn_manager.clone();
                    let limiter = self.rate_limiter.clone();
                    let stop_sig = stop_signal.clone();
                    let url = item.url.clone();
                    let referer = item.referer.clone();
                    let cookies = item.cookies.clone();
                    let buffer_size = self.config.buffer_size;

                    let handle = tokio::spawn(async move {
                        Self::run_segment_worker(
                            seg_id,
                            segs_ref,
                            active_ref,
                            total_dl_ref,
                            conn_mgr,
                            limiter,
                            stop_sig,
                            url,
                            referer,
                            cookies,
                            buffer_size,
                        )
                        .await;
                    });

                    worker_handles.push(handle);
                } else {
                    // No new segments to split right now; sleep briefly before re-checking
                    sleep(Duration::from_millis(50)).await;
                }
            } else {
                sleep(Duration::from_millis(50)).await;
            }
        }

        // Wait for active workers
        for h in worker_handles {
            let _ = h.await;
        }

        stop_signal.store(true, Ordering::Relaxed);
        let _ = monitor_handle.await;

        // Retrieve final segments
        item.segments = shared_segments.lock().await.clone();
        item.downloaded = total_downloaded.load(Ordering::Relaxed);

        if stop_signal.load(Ordering::Relaxed) && item.downloaded < item.file_size {
            item.status = DownloadStatus::Paused;
            self.state_manager.save_state(item)?;
            return Ok(());
        }

        // Assemble all segments
        let final_dest = item.final_destination();
        self.state_manager.log_event(
            &item.temp_dir,
            &format!("Assembling {} segments into {:?}", item.segments.len(), final_dest),
        )?;

        Assembler::assemble_segments(&item.segments, &final_dest, item.file_size)?;

        item.status = DownloadStatus::Complete;
        self.state_manager.save_state(item)?;
        self.state_manager.cleanup_task(&item.temp_dir)?;

        Ok(())
    }

    /// Worker task downloading a single segment chunk
    async fn run_segment_worker(
        seg_id: u32,
        shared_segments: Arc<Mutex<Vec<Segment>>>,
        active_workers: Arc<AtomicU32>,
        total_downloaded: Arc<AtomicU64>,
        conn_mgr: Arc<ConnectionManager>,
        limiter: Arc<RateLimiter>,
        stop_signal: Arc<AtomicBool>,
        url: String,
        referer: Option<String>,
        cookies: Option<String>,
        _buffer_size: usize,
    ) {
        // Read segment initial coordinates
        let (start_pos, end_pos, temp_file) = {
            let segs = shared_segments.lock().await;
            let s = segs.iter().find(|s| s.id == seg_id).unwrap();
            (s.current_pos, s.end_pos, s.temp_file.clone())
        };

        if start_pos > end_pos {
            active_workers.fetch_sub(1, Ordering::Relaxed);
            let mut segs = shared_segments.lock().await;
            if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id) {
                s.is_active = false;
            }
            return;
        }

        // Open chunk file and seek to current downloaded length
        let mut file = match OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(&temp_file)
        {
            Ok(f) => f,
            Err(_) => {
                active_workers.fetch_sub(1, Ordering::Relaxed);
                return;
            }
        };

        let req = conn_mgr.build_range_request(
            &url,
            start_pos,
            end_pos,
            referer.as_deref(),
            cookies.as_deref(),
        );

        match req.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let mut stream = response.bytes_stream();

                    while let Some(chunk_res) = stream.next().await {
                        if stop_signal.load(Ordering::Relaxed) {
                            break;
                        }

                        match chunk_res {
                            Ok(bytes) => {
                                let len = bytes.len();
                                limiter.acquire(len).await;

                                let (to_write, is_done) = {
                                    let mut segs = shared_segments.lock().await;
                                    if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id) {
                                        if s.current_pos > s.end_pos {
                                            (0, true)
                                        } else {
                                            let max_needed = (s.end_pos - s.current_pos + 1) as usize;
                                            let chunk_write = len.min(max_needed);
                                            s.downloaded += chunk_write as u64;
                                            s.current_pos += chunk_write as u64;
                                            let done = s.current_pos > s.end_pos;
                                            (chunk_write, done)
                                        }
                                    } else {
                                        (0, true)
                                    }
                                };

                                if to_write > 0 {
                                    if file.write_all(&bytes[..to_write]).is_ok() {
                                        total_downloaded.fetch_add(to_write as u64, Ordering::Relaxed);
                                    }
                                }

                                if is_done {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    let _ = file.flush();
                }
            }
            Err(_) => {}
        }

        active_workers.fetch_sub(1, Ordering::Relaxed);
        let mut segs = shared_segments.lock().await;
        if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id) {
            s.is_active = false;
        }
    }

    /// Single stream fallback for non-resumable servers
    async fn download_single_stream(
        &self,
        item: &mut DownloadItem,
        stop_signal: Arc<AtomicBool>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let dest = item.final_destination();
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&dest)?;

        let req = self.conn_manager.build_full_request(
            &item.url,
            item.referer.as_deref(),
            item.cookies.as_deref(),
        );

        let response = req.send().await?;
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;

        let mut last_instant = Instant::now();
        let mut last_bytes = 0u64;

        while let Some(chunk_res) = stream.next().await {
            if stop_signal.load(Ordering::Relaxed) {
                break;
            }

            let bytes = chunk_res?;
            let len = bytes.len();
            self.rate_limiter.acquire(len).await;
            file.write_all(&bytes)?;
            downloaded += len as u64;

            let now = Instant::now();
            let elapsed = now.duration_since(last_instant).as_secs_f64();
            if elapsed >= 0.5 {
                let speed = ((downloaded - last_bytes) as f64 / elapsed) as u64;
                last_bytes = downloaded;
                last_instant = now;

                let _ = self.progress_tx.send(ProgressUpdate {
                    download_id: item.id,
                    total_bytes: item.file_size,
                    downloaded_bytes: downloaded,
                    speed_bps: speed,
                    active_connections: 1,
                    segments: vec![SegmentProgress {
                        id: 0,
                        start_pos: 0,
                        current_pos: downloaded,
                        end_pos: item.file_size.max(downloaded),
                        downloaded,
                        is_active: true,
                    }],
                    status: DownloadStatus::Downloading,
                });
            }
        }

        file.flush()?;
        item.downloaded = downloaded;
        item.status = DownloadStatus::Complete;
        self.state_manager.save_state(item)?;

        Ok(())
    }

    /// Returns chunk file path: {filename} for chunk 0, {filename}{id} for subsequent chunks
    fn get_chunk_filepath(temp_dir: &Path, filename: &str, chunk_id: u32) -> PathBuf {
        if chunk_id == 0 {
            temp_dir.join(filename)
        } else {
            temp_dir.join(format!("{}{}", filename, chunk_id))
        }
    }
}
