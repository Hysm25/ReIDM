use crate::categories;
use crate::config::Config;
use crate::engine::connection::ConnectionManager;
use crate::engine::rate_limiter::RateLimiter;
use crate::engine::segmenter::{DynamicSegmenter, ProgressUpdate};
use crate::scheduler::{QueueConfig, QueueManager};
use crate::state::StateManager;
use crate::types::{DownloadItem, DownloadStatus};
use crate::ui::dialogs::add_url::AddUrlDialog;
use crate::ui::dialogs::complete_dialog::{CompleteDialogAction, DownloadCompleteDialog};
use crate::ui::dialogs::download_info::{DownloadInfoAction, DownloadInfoDialog};
use crate::ui::dialogs::options_dialog::{OptionsAction, OptionsDialog};
use crate::ui::dialogs::progress_dialog::{ProgressDialog, ProgressDialogAction};
use crate::ui::dialogs::scheduler_dialog::{SchedulerAction, SchedulerDialog};
use crate::ui::theme::IdmTheme;
use eframe::egui::{self, Sense, Stroke, Vec2};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct ReIdmApp {
    config: Arc<Config>,
    conn_manager: Arc<ConnectionManager>,
    state_manager: Arc<StateManager>,
    rate_limiter: Arc<RateLimiter>,
    queue_manager: Arc<QueueManager>,
    tokio_handle: tokio::runtime::Handle,

    // Downloads state
    downloads: Vec<DownloadItem>,
    selected_id: Option<u64>,
    active_updates: HashMap<u64, ProgressUpdate>,
    stop_signals: HashMap<u64, Arc<AtomicBool>>,

    // Category filter
    selected_category: String,

    // Channels
    rx_progress: std::sync::mpsc::Receiver<ProgressUpdate>,
    tx_progress: std::sync::mpsc::Sender<ProgressUpdate>,

    // Dialogs
    add_url_dialog: AddUrlDialog,
    download_info_dialog: DownloadInfoDialog,
    progress_dialog: ProgressDialog,
    complete_dialog: DownloadCompleteDialog,
    options_dialog: OptionsDialog,
    scheduler_dialog: SchedulerDialog,
    show_about: bool,
}

impl ReIdmApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: Config,
        tokio_handle: tokio::runtime::Handle,
    ) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        IdmTheme::apply(&cc.egui_ctx);

        let conn_manager = Arc::new(
            ConnectionManager::new(config.user_agent.clone(), config.timeout_secs)
                .expect("Failed to initialize connection manager"),
        );
        let state_manager = Arc::new(StateManager::new(config.temp_dir.clone()));
        let rate_limiter = Arc::new(RateLimiter::new(config.speed_limit));
        let queue_manager = Arc::new(QueueManager::new(QueueConfig::default()));
        let (tx_progress, rx_progress) = std::sync::mpsc::channel();

        // Load existing paused or queued downloads from temp state dir
        let existing = state_manager.list_paused_tasks();

        Self {
            config: Arc::new(config),
            conn_manager,
            state_manager,
            rate_limiter,
            queue_manager,
            tokio_handle,
            downloads: existing,
            selected_id: None,
            active_updates: HashMap::new(),
            stop_signals: HashMap::new(),
            selected_category: "All Downloads".to_string(),
            rx_progress,
            tx_progress,
            add_url_dialog: AddUrlDialog::default(),
            download_info_dialog: DownloadInfoDialog::default(),
            progress_dialog: ProgressDialog::default(),
            complete_dialog: DownloadCompleteDialog::default(),
            options_dialog: OptionsDialog::default(),
            scheduler_dialog: SchedulerDialog::default(),
            show_about: false,
        }
    }

    /// Spawns a background download task using DynamicSegmenter
    fn start_download_task(&mut self, mut item: DownloadItem) {
        let id = item.id;
        let stop_signal = Arc::new(AtomicBool::new(false));
        self.stop_signals.insert(id, stop_signal.clone());

        // Update item status in table
        item.status = DownloadStatus::Downloading;
        if let Some(pos) = self.downloads.iter().position(|d| d.id == id) {
            self.downloads[pos] = item.clone();
        } else {
            self.downloads.push(item.clone());
        }

        let config = self.config.clone();
        let conn_mgr = self.conn_manager.clone();
        let state_mgr = self.state_manager.clone();
        let limiter = self.rate_limiter.clone();
        let tx = self.tx_progress.clone();

        self.tokio_handle.spawn(async move {
            let (segmenter, mut progress_rx) = DynamicSegmenter::new(config, conn_mgr, state_mgr, limiter);

            // Forward progress to GUI channel
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                while let Ok(update) = progress_rx.recv().await {
                    let _ = tx_clone.send(update);
                }
            });

            let res = segmenter.download(&mut item, stop_signal).await;
            if res.is_ok() && item.status == DownloadStatus::Complete {
                let _ = tx.send(ProgressUpdate {
                    download_id: item.id,
                    total_bytes: item.file_size,
                    downloaded_bytes: item.file_size,
                    speed_bps: 0,
                    active_connections: 0,
                    segments: vec![],
                    status: DownloadStatus::Complete,
                });
            }
        });
    }

    /// Pauses an active download
    fn pause_download(&mut self, id: u64) {
        if let Some(stop) = self.stop_signals.get(&id) {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(pos) = self.downloads.iter().position(|d| d.id == id) {
            self.downloads[pos].status = DownloadStatus::Paused;
        }
    }

    /// Resumes a paused download
    fn resume_download(&mut self, id: u64) {
        if let Some(item) = self.downloads.iter().find(|d| d.id == id).cloned() {
            if item.status == DownloadStatus::Paused || item.status == DownloadStatus::Idle {
                self.start_download_task(item);
            }
        }
    }

    /// Stops all downloads
    fn stop_all(&mut self) {
        for stop in self.stop_signals.values() {
            stop.store(true, Ordering::Relaxed);
        }
        for item in &mut self.downloads {
            if item.status == DownloadStatus::Downloading {
                item.status = DownloadStatus::Paused;
            }
        }
    }

    /// Removes a download
    fn remove_download(&mut self, id: u64) {
        self.pause_download(id);
        if let Some(pos) = self.downloads.iter().position(|d| d.id == id) {
            let item = self.downloads.remove(pos);
            let _ = self.state_manager.cleanup_task(&item.temp_dir);
        }
        if self.selected_id == Some(id) {
            self.selected_id = None;
        }
    }

    /// Removes all completed downloads from table
    fn remove_all_completed(&mut self) {
        self.downloads.retain(|d| d.status != DownloadStatus::Complete);
    }
}

impl eframe::App for ReIdmApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain incoming progress updates from Tokio background tasks
        while let Ok(update) = self.rx_progress.try_recv() {
            let id = update.download_id;
            let status = update.status;
            self.active_updates.insert(id, update);

            if let Some(pos) = self.downloads.iter().position(|d| d.id == id) {
                if let Some(p) = self.active_updates.get(&id) {
                    self.downloads[pos].downloaded = p.downloaded_bytes;
                    self.downloads[pos].file_size = p.total_bytes;
                }
                if status == DownloadStatus::Complete {
                    self.downloads[pos].status = DownloadStatus::Complete;
                    self.complete_dialog.item = Some(self.downloads[pos].clone());
                    self.complete_dialog.is_open = true;
                }
            }
        }

        // 1. Top Menu Bar (Tasks, File, Downloads, View, Help)
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Tasks", |ui| {
                    if ui.button("Add new download").clicked() {
                        self.add_url_dialog.is_open = true;
                        ui.close_menu();
                    }
                    if ui.button("Show drop target").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("File", |ui| {
                    let has_selected = self.selected_id.is_some();
                    if ui.add_enabled(has_selected, egui::Button::new("Download Now")).clicked() {
                        if let Some(id) = self.selected_id {
                            self.resume_download(id);
                        }
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_selected, egui::Button::new("Stop Download")).clicked() {
                        if let Some(id) = self.selected_id {
                            self.pause_download(id);
                        }
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_selected, egui::Button::new("Remove")).clicked() {
                        if let Some(id) = self.selected_id {
                            self.remove_download(id);
                        }
                        ui.close_menu();
                    }
                });

                ui.menu_button("Downloads", |ui| {
                    if ui.button("Pause All").clicked() {
                        self.stop_all();
                        ui.close_menu();
                    }
                    if ui.button("Stop All").clicked() {
                        self.stop_all();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Delete All Completed").clicked() {
                        self.remove_all_completed();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Scheduler").clicked() {
                        self.scheduler_dialog.is_open = true;
                        ui.close_menu();
                    }
                    if ui.button("Options").clicked() {
                        self.options_dialog.is_open = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("All Downloads").clicked() {
                        self.selected_category = "All Downloads".to_string();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About ReIDM").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });

        // 2. Toolbar Panel with 12 authentic IDM buttons
        egui::TopBottomPanel::top("toolbar_panel")
            .frame(egui::Frame::none().fill(IdmTheme::TOOLBAR_BG))
            .show(ctx, |ui| {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);

                    // Helper for toolbar button: 52px width, centered icon + label
                    let tool_btn = |ui: &mut egui::Ui, name: &str, label: &str| -> bool {
                        let btn_rect = ui.allocate_exact_size(Vec2::new(56.0, 50.0), Sense::click());
                        let hovered = ui.rect_contains_pointer(btn_rect.0);
                        let clicked = btn_rect.1.clicked();

                        let painter = ui.painter();
                        if hovered {
                            painter.rect_filled(btn_rect.0, 2.0, IdmTheme::TOOLBAR_HOVER);
                            painter.rect_stroke(btn_rect.0, 2.0, Stroke::new(1.0_f32, IdmTheme::TOOLBAR_BORDER));
                        }

                        // Icon path
                        let icon_uri = match name {
                            "add_url" => egui::include_image!("../../assets/idm_gui/toolbar/add_url.png"),
                            "resume" => egui::include_image!("../../assets/idm_gui/toolbar/resume.png"),
                            "stop" => egui::include_image!("../../assets/idm_gui/toolbar/stop.png"),
                            "stop_all" => egui::include_image!("../../assets/idm_gui/toolbar/stop_all.png"),
                            "delete" => egui::include_image!("../../assets/idm_gui/toolbar/delete.png"),
                            "delete_completed" => egui::include_image!("../../assets/idm_gui/toolbar/delete_completed.png"),
                            "options" => egui::include_image!("../../assets/idm_gui/toolbar/options.png"),
                            "scheduler" => egui::include_image!("../../assets/idm_gui/toolbar/scheduler.png"),
                            "start_queue" => egui::include_image!("../../assets/idm_gui/toolbar/start_queue.png"),
                            "stop_queue" => egui::include_image!("../../assets/idm_gui/toolbar/stop_queue.png"),
                            _ => egui::include_image!("../../assets/idm_gui/toolbar/tell_friend.png"),
                        };

                        let icon_rect = egui::Rect::from_center_size(
                            egui::pos2(btn_rect.0.center().x, btn_rect.0.top() + 18.0),
                            Vec2::new(32.0, 28.0),
                        );
                        egui::Image::new(icon_uri).paint_at(ui, icon_rect);

                        // Label underneath
                        let label_rect = egui::Rect::from_center_size(
                            egui::pos2(btn_rect.0.center().x, btn_rect.0.bottom() - 8.0),
                            Vec2::new(54.0, 14.0),
                        );
                        painter.text(
                            label_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            egui::FontId::proportional(10.5),
                            IdmTheme::BLACK,
                        );

                        clicked
                    };

                    if tool_btn(ui, "add_url", "Add URL") {
                        self.add_url_dialog.is_open = true;
                    }

                    if tool_btn(ui, "resume", "Resume") {
                        if let Some(id) = self.selected_id {
                            self.resume_download(id);
                        }
                    }

                    if tool_btn(ui, "stop", "Stop") {
                        if let Some(id) = self.selected_id {
                            self.pause_download(id);
                        }
                    }

                    if tool_btn(ui, "stop_all", "Stop All") {
                        self.stop_all();
                    }

                    if tool_btn(ui, "delete", "Delete") {
                        if let Some(id) = self.selected_id {
                            self.remove_download(id);
                        }
                    }

                    if tool_btn(ui, "delete_completed", "Delete All") {
                        self.remove_all_completed();
                    }

                    ui.separator();

                    if tool_btn(ui, "options", "Options") {
                        self.options_dialog.is_open = true;
                    }

                    if tool_btn(ui, "scheduler", "Scheduler") {
                        self.scheduler_dialog.is_open = true;
                    }

                    if tool_btn(ui, "start_queue", "Start Queue") {
                        self.queue_manager.start();
                    }

                    if tool_btn(ui, "stop_queue", "Stop Queue") {
                        self.queue_manager.stop();
                    }

                    if tool_btn(ui, "tell_friend", "Tell a Friend") {
                        self.show_about = true;
                    }
                });
                ui.add_space(2.0);
            });

        // 3. Bottom Status Bar Panel
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::none().fill(IdmTheme::STATUS_BAR_BG))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let active_count = self.downloads.iter().filter(|d| d.status == DownloadStatus::Downloading).count();
                    let total_speed: u64 = self.active_updates.values().map(|u| u.speed_bps).sum();

                    ui.label(format!("{} download(s)", self.downloads.len()));
                    ui.separator();
                    ui.label(format!("Active: {}", active_count));
                    ui.separator();

                    let speed_str = if total_speed >= 1_048_576 {
                        format!("{:.2} MB/s", total_speed as f64 / 1_048_576.0)
                    } else {
                        format!("{:.1} KB/s", total_speed as f64 / 1024.0)
                    };
                    ui.label(format!("Total Speed: {}", speed_str));
                    ui.separator();

                    let limit_str = if let Some(l) = self.config.speed_limit {
                        format!("{:.0} KB/s", l as f64 / 1024.0)
                    } else {
                        "OFF".to_string()
                    };
                    ui.label(format!("Speed Limiter: {}", limit_str));
                });
            });

        // 4. Left Sidebar (Categories Tree)
        egui::SidePanel::left("categories_panel")
            .default_width(170.0)
            .width_range(140.0..=240.0)
            .frame(egui::Frame::none().fill(IdmTheme::WHITE))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.strong("  Categories");
                ui.separator();

                let categories = [
                    ("All Downloads", "all"),
                    ("Compressed", "compressed"),
                    ("Documents", "documents"),
                    ("Music", "music"),
                    ("Programs", "programs"),
                    ("Video", "video"),
                    ("Finished", "finished"),
                    ("Unfinished", "unfinished"),
                ];

                for (cat_name, _icon_key) in categories {
                    let count = match cat_name {
                        "All Downloads" => self.downloads.len(),
                        "Finished" => self.downloads.iter().filter(|d| d.status == DownloadStatus::Complete).count(),
                        "Unfinished" => self.downloads.iter().filter(|d| d.status != DownloadStatus::Complete).count(),
                        other => self.downloads.iter().filter(|d| d.category == other).count(),
                    };

                    let is_sel = self.selected_category == cat_name;
                    let label_text = format!("{} ({})", cat_name, count);

                    if ui.selectable_label(is_sel, label_text).clicked() {
                        self.selected_category = cat_name.to_string();
                    }
                }

                ui.add_space(10.0);
                ui.strong("  Queues");
                ui.separator();
                let _ = ui.selectable_label(false, "Main download queue");
                let _ = ui.selectable_label(false, "Synchronization queue");
            });

        // 5. Central Area: Main Downloads Table
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(IdmTheme::WHITE))
            .show(ctx, |ui| {
                let filtered: Vec<&DownloadItem> = self
                    .downloads
                    .iter()
                    .filter(|d| match self.selected_category.as_str() {
                        "All Downloads" => true,
                        "Finished" => d.status == DownloadStatus::Complete,
                        "Unfinished" => d.status != DownloadStatus::Complete,
                        cat => d.category == cat,
                    })
                    .collect();

                egui::ScrollArea::both().show(ui, |ui| {
                    egui::Grid::new("downloads_grid")
                        .striped(true)
                        .min_col_width(80.0)
                        .show(ui, |ui| {
                            // Table Headers (Classic Win32 style)
                            ui.strong("File Name");
                            ui.strong("Q");
                            ui.strong("Size");
                            ui.strong("Status");
                            ui.strong("Time Left");
                            ui.strong("Transfer Rate");
                            ui.strong("Last Try Date");
                            ui.strong("Description");
                            ui.end_row();

                            for item in filtered {
                                let is_selected = self.selected_id == Some(item.id);
                                let speed = self.active_updates.get(&item.id).map(|u| u.speed_bps).unwrap_or(0);
                                let downloaded = self.active_updates.get(&item.id).map(|u| u.downloaded_bytes).unwrap_or(item.downloaded);
                                let total = self.active_updates.get(&item.id).map(|u| u.total_bytes).unwrap_or(item.file_size);

                                let percent = if total > 0 {
                                    (downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
                                } else {
                                    0.0
                                };

                                // File Name with selection handling & double click
                                let name_label = ui.selectable_label(is_selected, &item.file_name);
                                if name_label.clicked() {
                                    self.selected_id = Some(item.id);
                                }
                                if name_label.double_clicked() {
                                    self.progress_dialog.download_id = item.id;
                                    self.progress_dialog.is_open = true;
                                }

                                // Q (Queue indicator from screenshot s-1.png / main-w.png)
                                let q_str = if item.status == DownloadStatus::Idle { "📁" } else { "" };
                                ui.label(q_str);

                                // Size
                                let size_str = if total > 0 {
                                    format!("{:.2} MB", total as f64 / 1_048_576.0)
                                } else {
                                    "Unknown".to_string()
                                };
                                ui.label(size_str);

                                // Status
                                let (status_str, status_color) = match item.status {
                                    DownloadStatus::Downloading => (format!("Downloading ({:.1}%)", percent), IdmTheme::STATUS_DOWNLOADING),
                                    DownloadStatus::Complete => ("Complete".to_string(), IdmTheme::STATUS_COMPLETE),
                                    DownloadStatus::Paused => (format!("Paused ({:.1}%)", percent), IdmTheme::STATUS_PAUSED),
                                    DownloadStatus::Idle => ("Queued".to_string(), IdmTheme::BLACK),
                                    DownloadStatus::Connecting => ("Connecting...".to_string(), IdmTheme::STATUS_DOWNLOADING),
                                    DownloadStatus::Error => ("Error".to_string(), IdmTheme::STATUS_ERROR),
                                };
                                ui.colored_label(status_color, status_str);

                                // Time Left
                                let time_left = if speed > 0 && total > downloaded {
                                    let secs = (total - downloaded) / speed;
                                    format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
                                } else if item.status == DownloadStatus::Complete {
                                    "00:00:00".to_string()
                                } else {
                                    "--:--:--".to_string()
                                };
                                ui.label(time_left);

                                // Transfer Rate
                                let rate_str = if speed > 0 {
                                    format!("{:.1} KB/s", speed as f64 / 1024.0)
                                } else {
                                    "".to_string()
                                };
                                ui.label(rate_str);

                                // Date
                                ui.label(item.created_at.format("%Y-%m-%d %H:%M").to_string());

                                // Category / Description
                                ui.label(&item.category);
                                ui.end_row();
                            }
                        });
                });
            });

        // 6. Handle Dialogs
        // Add URL Dialog
        if let Some(url) = self.add_url_dialog.show(ctx) {
            // Probe URL and prepare DownloadInfoDialog
            let cm = self.conn_manager.clone();
            let cfg = self.config.clone();
            let (tx_info, rx_info) = std::sync::mpsc::channel();

            self.tokio_handle.spawn(async move {
                if let Ok(probe) = cm.probe(&url, None, None).await {
                    let (dest, cat) = categories::resolve_destination(&cfg.download_dir, &probe.file_name);
                    let _ = tx_info.send((url, probe.file_name, dest, cat, probe.file_size, probe.resumable));
                }
            });

            // Receive probe info and open DownloadInfoDialog
            if let Ok((u, fname, dest, cat, size, resumable)) = rx_info.recv_timeout(std::time::Duration::from_secs(3)) {
                self.download_info_dialog.url = u;
                self.download_info_dialog.file_name = fname;
                self.download_info_dialog.save_path = dest;
                self.download_info_dialog.category = cat;
                self.download_info_dialog.file_size = size;
                self.download_info_dialog.resumable = resumable;
                self.download_info_dialog.is_open = true;
            }
        }

        // Download Info Dialog
        if let Some(action) = self.download_info_dialog.show(ctx) {
            match action {
                DownloadInfoAction::StartDownload => {
                    let download_id = chrono::Utc::now().timestamp_millis() as u64;
                    let task_dir = self.state_manager.get_task_dir(&self.download_info_dialog.file_name, download_id);

                    let item = DownloadItem {
                        id: download_id,
                        url: self.download_info_dialog.url.clone(),
                        file_name: self.download_info_dialog.file_name.clone(),
                        local_path: self.download_info_dialog.save_path.clone(),
                        category: self.download_info_dialog.category.clone(),
                        file_size: self.download_info_dialog.file_size,
                        downloaded: 0,
                        status: DownloadStatus::Downloading,
                        resumable: self.download_info_dialog.resumable,
                        user_agent: self.config.user_agent.clone(),
                        referer: None,
                        cookies: None,
                        etag: None,
                        last_modified: None,
                        content_type: None,
                        created_at: chrono::Utc::now(),
                        temp_dir: task_dir,
                        segments: Vec::new(),
                    };

                    self.progress_dialog.download_id = download_id;
                    self.progress_dialog.is_open = true;
                    self.start_download_task(item);
                }
                DownloadInfoAction::DownloadLater => {
                    let download_id = chrono::Utc::now().timestamp_millis() as u64;
                    let task_dir = self.state_manager.get_task_dir(&self.download_info_dialog.file_name, download_id);

                    let item = DownloadItem {
                        id: download_id,
                        url: self.download_info_dialog.url.clone(),
                        file_name: self.download_info_dialog.file_name.clone(),
                        local_path: self.download_info_dialog.save_path.clone(),
                        category: self.download_info_dialog.category.clone(),
                        file_size: self.download_info_dialog.file_size,
                        downloaded: 0,
                        status: DownloadStatus::Idle,
                        resumable: self.download_info_dialog.resumable,
                        user_agent: self.config.user_agent.clone(),
                        referer: None,
                        cookies: None,
                        etag: None,
                        last_modified: None,
                        content_type: None,
                        created_at: chrono::Utc::now(),
                        temp_dir: task_dir,
                        segments: Vec::new(),
                    };
                    self.downloads.push(item);
                }
                DownloadInfoAction::Cancel => {}
            }
        }

        // Progress Dialog (Dialog 103 / 363 with SegmentVisualizer)
        if self.progress_dialog.is_open {
            let pid = self.progress_dialog.download_id;
            if let Some(item) = self.downloads.iter().find(|d| d.id == pid).cloned() {
                let update = self.active_updates.get(&pid);
                if let Some(p_action) = self.progress_dialog.show(ctx, &item, update) {
                    match p_action {
                        ProgressDialogAction::Pause => self.pause_download(pid),
                        ProgressDialogAction::Resume => self.resume_download(pid),
                        ProgressDialogAction::Cancel => self.pause_download(pid),
                    }
                }
            }
        }

        // Complete Dialog
        if let Some(c_action) = self.complete_dialog.show(ctx) {
            match c_action {
                CompleteDialogAction::Open => {
                    if let Some(ref it) = self.complete_dialog.item {
                        let _ = std::process::Command::new("xdg-open")
                            .arg(it.final_destination())
                            .spawn();
                    }
                }
                CompleteDialogAction::OpenFolder => {
                    if let Some(ref it) = self.complete_dialog.item {
                        let _ = std::process::Command::new("xdg-open")
                            .arg(&it.local_path)
                            .spawn();
                    }
                }
                CompleteDialogAction::Close => {}
            }
        }

        // Options Dialog
        if let Some(opt_action) = self.options_dialog.show(ctx) {
            if let OptionsAction::Save(new_cfg) = opt_action {
                self.config = Arc::new(new_cfg);
            }
        }

        // Scheduler Dialog
        if let Some(sched_action) = self.scheduler_dialog.show(ctx) {
            match sched_action {
                SchedulerAction::StartQueue => self.queue_manager.start(),
                SchedulerAction::StopQueue => self.queue_manager.stop(),
                SchedulerAction::Close => {}
            }
        }

        // About Dialog (Dialog 100)
        if self.show_about {
            egui::Window::new("About ReIDM")
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let app_icon = egui::include_image!("../../assets/idm_gui/app_icon.png");
                        ui.add(egui::Image::new(app_icon).fit_to_exact_size(Vec2::new(48.0, 48.0)));
                        ui.vertical(|ui| {
                            ui.heading("ReIDM - Internet Download Manager");
                            ui.label("Version 6.43 (Rust Re-Implementation)");
                            ui.label("High-Performance Dynamic In-Half Segmentation");
                        });
                    });
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        self.show_about = false;
                    }
                });
        }

        // Request continuous repaint while downloads are active for smooth 60 FPS progress updates
        let any_downloading = self.downloads.iter().any(|d| d.status == DownloadStatus::Downloading);
        if any_downloading || self.progress_dialog.is_open {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
