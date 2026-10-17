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

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum ToolbarTheme {
    Neon,
    Classic3D,
}

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

    // Category filter and view state
    selected_category: String,
    toolbar_theme: ToolbarTheme,
    show_categories: bool,
    cat_all_expanded: bool,
    cat_queues_expanded: bool,

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
            toolbar_theme: ToolbarTheme::Neon,
            show_categories: true,
            cat_all_expanded: true,
            cat_queues_expanded: true,
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

        // 1. Top Menu Bar matching MENU 129
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("Tasks", |ui| {
                    if ui.button("Add new download").clicked() {
                        self.add_url_dialog.is_open = true;
                        ui.close_menu();
                    }
                    if ui.button("Add batch download").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Add batch download from clipboard").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Run site grabber").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Show drop target").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.menu_button("Export", |ui| {
                        if ui.button("To IDM export file").clicked() { ui.close_menu(); }
                        if ui.button("To text file").clicked() { ui.close_menu(); }
                    });
                    ui.menu_button("Import", |ui| {
                        if ui.button("From IDM export file").clicked() { ui.close_menu(); }
                        if ui.button("From text file").clicked() { ui.close_menu(); }
                    });
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                let has_selected = self.selected_id.is_some();
                let selected_item = self.selected_id.and_then(|id| self.downloads.iter().find(|d| d.id == id));
                let is_selected_downloading = selected_item.map(|d| d.status == DownloadStatus::Downloading).unwrap_or(false);

                ui.menu_button("File", |ui| {
                    if ui.add_enabled(has_selected && !is_selected_downloading, egui::Button::new("Download Now")).clicked() {
                        if let Some(id) = self.selected_id {
                            self.resume_download(id);
                        }
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_selected && is_selected_downloading, egui::Button::new("Stop Download")).clicked() {
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
                    if ui.add_enabled(has_selected, egui::Button::new("Redownload")).clicked() {
                        if let Some(id) = self.selected_id {
                            self.resume_download(id);
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
                    let has_completed = self.downloads.iter().any(|d| d.status == DownloadStatus::Complete);
                    if ui.add_enabled(has_completed, egui::Button::new("Delete All Completed")).clicked() {
                        self.remove_all_completed();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Scheduler").clicked() {
                        self.scheduler_dialog.is_open = true;
                        ui.close_menu();
                    }
                    ui.menu_button("Start queue", |ui| {
                        if ui.button("Start").clicked() {
                            self.queue_manager.start();
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Stop queue", |ui| {
                        if ui.button("Stop").clicked() {
                            self.queue_manager.stop();
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    ui.menu_button("Speed Limiter", |ui| {
                        if ui.button("Turn on").clicked() {
                            self.rate_limiter.set_limit(self.config.speed_limit);
                            ui.close_menu();
                        }
                        if ui.button("Turn off").clicked() {
                            self.rate_limiter.set_limit(None);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Settings...").clicked() {
                            self.options_dialog.active_tab = crate::ui::dialogs::options_dialog::OptionsTab::Connection;
                            self.options_dialog.is_open = true;
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    if ui.button("Options").clicked() {
                        self.options_dialog.is_open = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    let cat_label = if self.show_categories { "Hide categories" } else { "Show categories" };
                    if ui.button(cat_label).clicked() {
                        self.show_categories = !self.show_categories;
                        ui.close_menu();
                    }
                    ui.menu_button("Toolbar", |ui| {
                        if ui.selectable_label(self.toolbar_theme == ToolbarTheme::Neon, "Neon").clicked() {
                            self.toolbar_theme = ToolbarTheme::Neon;
                            ui.close_menu();
                        }
                        if ui.selectable_label(self.toolbar_theme == ToolbarTheme::Classic3D, "3D Style").clicked() {
                            self.toolbar_theme = ToolbarTheme::Classic3D;
                            ui.close_menu();
                        }
                    });
                    if ui.button("Dark Mode support").clicked() {
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Help contents").clicked() { ui.close_menu(); }
                    if ui.button("Tutorials").clicked() { ui.close_menu(); }
                    if ui.button("Scheduler and queues").clicked() {
                        self.scheduler_dialog.is_open = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("IDM Home Page").clicked() {
                        let _ = std::process::Command::new("xdg-open").arg("http://www.internetdownloadmanager.com").spawn();
                        ui.close_menu();
                    }
                    if ui.button("Contact IDM Support").clicked() {
                        let _ = std::process::Command::new("xdg-open").arg("http://www.internetdownloadmanager.com/contact_us.html").spawn();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("About IDM").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                    if ui.button("Tell a friend").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("Registration", |ui| {
                    if ui.button("Order online").clicked() {
                        let _ = std::process::Command::new("xdg-open").arg("https://secure.internetdownloadmanager.com/buy_idm.html").spawn();
                        ui.close_menu();
                    }
                    if ui.button("Registration").clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });
            });
        });

        // 2. Authentic Toolbar Panel with 12 IDM buttons
        let toolbar_bg = match self.toolbar_theme {
            ToolbarTheme::Neon => IdmTheme::TOOLBAR_BG,
            ToolbarTheme::Classic3D => IdmTheme::TOOLBAR_BG_CLASSIC,
        };

        egui::TopBottomPanel::top("toolbar_panel")
            .frame(egui::Frame::none().fill(toolbar_bg))
            .show(ctx, |ui| {
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(3.0, 0.0);

                    let is_neon = self.toolbar_theme == ToolbarTheme::Neon;

                    let tool_btn = |ui: &mut egui::Ui, name: &str, label: &str, enabled: bool| -> bool {
                        let btn_rect = ui.allocate_exact_size(Vec2::new(60.0, 52.0), if enabled { Sense::click() } else { Sense::hover() });
                        let hovered = enabled && ui.rect_contains_pointer(btn_rect.0);
                        let clicked = enabled && btn_rect.1.clicked();

                        let painter = ui.painter();
                        if hovered {
                            painter.rect_filled(btn_rect.0, 2.0, IdmTheme::TOOLBAR_HOVER);
                            painter.rect_stroke(btn_rect.0, 2.0, Stroke::new(1.0_f32, IdmTheme::TOOLBAR_BORDER));
                        }

                        // Determine icon image
                        let icon_source = if is_neon {
                            if !enabled {
                                match name {
                                    "add_url" => egui::include_image!("../../assets/idm_gui/toolbar_neon/add_url_disabled.png"),
                                    "resume" => egui::include_image!("../../assets/idm_gui/toolbar_neon/resume_disabled.png"),
                                    "stop" => egui::include_image!("../../assets/idm_gui/toolbar_neon/stop_disabled.png"),
                                    "stop_all" => egui::include_image!("../../assets/idm_gui/toolbar_neon/stop_all_disabled.png"),
                                    "delete" => egui::include_image!("../../assets/idm_gui/toolbar_neon/delete_disabled.png"),
                                    "delete_completed" => egui::include_image!("../../assets/idm_gui/toolbar_neon/delete_completed_disabled.png"),
                                    "options" => egui::include_image!("../../assets/idm_gui/toolbar_neon/options_disabled.png"),
                                    "scheduler" => egui::include_image!("../../assets/idm_gui/toolbar_neon/scheduler_disabled.png"),
                                    "start_queue" => egui::include_image!("../../assets/idm_gui/toolbar_neon/start_queue_disabled.png"),
                                    "stop_queue" => egui::include_image!("../../assets/idm_gui/toolbar_neon/stop_queue_disabled.png"),
                                    "grabber" => egui::include_image!("../../assets/idm_gui/toolbar_neon/grabber_disabled.png"),
                                    _ => egui::include_image!("../../assets/idm_gui/toolbar_neon/tell_friend_disabled.png"),
                                }
                            } else if hovered {
                                match name {
                                    "add_url" => egui::include_image!("../../assets/idm_gui/toolbar_neon/add_url_hot.png"),
                                    "resume" => egui::include_image!("../../assets/idm_gui/toolbar_neon/resume_hot.png"),
                                    "stop" => egui::include_image!("../../assets/idm_gui/toolbar_neon/stop_hot.png"),
                                    "stop_all" => egui::include_image!("../../assets/idm_gui/toolbar_neon/stop_all_hot.png"),
                                    "delete" => egui::include_image!("../../assets/idm_gui/toolbar_neon/delete_hot.png"),
                                    "delete_completed" => egui::include_image!("../../assets/idm_gui/toolbar_neon/delete_completed_hot.png"),
                                    "options" => egui::include_image!("../../assets/idm_gui/toolbar_neon/options_hot.png"),
                                    "scheduler" => egui::include_image!("../../assets/idm_gui/toolbar_neon/scheduler_hot.png"),
                                    "start_queue" => egui::include_image!("../../assets/idm_gui/toolbar_neon/start_queue_hot.png"),
                                    "stop_queue" => egui::include_image!("../../assets/idm_gui/toolbar_neon/stop_queue_hot.png"),
                                    "grabber" => egui::include_image!("../../assets/idm_gui/toolbar_neon/grabber_hot.png"),
                                    _ => egui::include_image!("../../assets/idm_gui/toolbar_neon/tell_friend_hot.png"),
                                }
                            } else {
                                match name {
                                    "add_url" => egui::include_image!("../../assets/idm_gui/toolbar_neon/add_url.png"),
                                    "resume" => egui::include_image!("../../assets/idm_gui/toolbar_neon/resume.png"),
                                    "stop" => egui::include_image!("../../assets/idm_gui/toolbar_neon/stop.png"),
                                    "stop_all" => egui::include_image!("../../assets/idm_gui/toolbar_neon/stop_all.png"),
                                    "delete" => egui::include_image!("../../assets/idm_gui/toolbar_neon/delete.png"),
                                    "delete_completed" => egui::include_image!("../../assets/idm_gui/toolbar_neon/delete_completed.png"),
                                    "options" => egui::include_image!("../../assets/idm_gui/toolbar_neon/options.png"),
                                    "scheduler" => egui::include_image!("../../assets/idm_gui/toolbar_neon/scheduler.png"),
                                    "start_queue" => egui::include_image!("../../assets/idm_gui/toolbar_neon/start_queue.png"),
                                    "stop_queue" => egui::include_image!("../../assets/idm_gui/toolbar_neon/stop_queue.png"),
                                    "grabber" => egui::include_image!("../../assets/idm_gui/toolbar_neon/grabber.png"),
                                    _ => egui::include_image!("../../assets/idm_gui/toolbar_neon/tell_friend.png"),
                                }
                            }
                        } else {
                            if hovered && enabled {
                                match name {
                                    "add_url" => egui::include_image!("../../assets/idm_gui/toolbar_3d/add_url_hot.png"),
                                    "resume" => egui::include_image!("../../assets/idm_gui/toolbar_3d/resume_hot.png"),
                                    "stop" => egui::include_image!("../../assets/idm_gui/toolbar_3d/stop_hot.png"),
                                    "stop_all" => egui::include_image!("../../assets/idm_gui/toolbar_3d/stop_all_hot.png"),
                                    "delete" => egui::include_image!("../../assets/idm_gui/toolbar_3d/delete_hot.png"),
                                    "delete_completed" => egui::include_image!("../../assets/idm_gui/toolbar_3d/delete_completed_hot.png"),
                                    "options" => egui::include_image!("../../assets/idm_gui/toolbar_3d/options_hot.png"),
                                    "scheduler" => egui::include_image!("../../assets/idm_gui/toolbar_3d/scheduler_hot.png"),
                                    "start_queue" => egui::include_image!("../../assets/idm_gui/toolbar_3d/start_queue_hot.png"),
                                    "stop_queue" => egui::include_image!("../../assets/idm_gui/toolbar_3d/stop_queue_hot.png"),
                                    "grabber" => egui::include_image!("../../assets/idm_gui/toolbar_3d/grabber_hot.png"),
                                    _ => egui::include_image!("../../assets/idm_gui/toolbar_3d/tell_friend_hot.png"),
                                }
                            } else {
                                match name {
                                    "add_url" => egui::include_image!("../../assets/idm_gui/toolbar_3d/add_url.png"),
                                    "resume" => egui::include_image!("../../assets/idm_gui/toolbar_3d/resume.png"),
                                    "stop" => egui::include_image!("../../assets/idm_gui/toolbar_3d/stop.png"),
                                    "stop_all" => egui::include_image!("../../assets/idm_gui/toolbar_3d/stop_all.png"),
                                    "delete" => egui::include_image!("../../assets/idm_gui/toolbar_3d/delete.png"),
                                    "delete_completed" => egui::include_image!("../../assets/idm_gui/toolbar_3d/delete_completed.png"),
                                    "options" => egui::include_image!("../../assets/idm_gui/toolbar_3d/options.png"),
                                    "scheduler" => egui::include_image!("../../assets/idm_gui/toolbar_3d/scheduler.png"),
                                    "start_queue" => egui::include_image!("../../assets/idm_gui/toolbar_3d/start_queue.png"),
                                    "stop_queue" => egui::include_image!("../../assets/idm_gui/toolbar_3d/stop_queue.png"),
                                    "grabber" => egui::include_image!("../../assets/idm_gui/toolbar_3d/grabber.png"),
                                    _ => egui::include_image!("../../assets/idm_gui/toolbar_3d/tell_friend.png"),
                                }
                            }
                        };

                        let icon_rect = egui::Rect::from_center_size(
                            egui::pos2(btn_rect.0.center().x, btn_rect.0.top() + 18.0),
                            Vec2::new(32.0, 28.0),
                        );
                        egui::Image::new(icon_source).paint_at(ui, icon_rect);

                        // Label underneath
                        let label_rect = egui::Rect::from_center_size(
                            egui::pos2(btn_rect.0.center().x, btn_rect.0.bottom() - 8.0),
                            Vec2::new(58.0, 14.0),
                        );
                        let text_color = if enabled { IdmTheme::BLACK } else { IdmTheme::WIN_DARK };
                        painter.text(
                            label_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            egui::FontId::proportional(10.5),
                            text_color,
                        );

                        clicked
                    };

                    let has_selected = self.selected_id.is_some();
                    let selected_item = self.selected_id.and_then(|id| self.downloads.iter().find(|d| d.id == id));
                    let is_downloading = selected_item.map(|d| d.status == DownloadStatus::Downloading).unwrap_or(false);
                    let can_resume = has_selected && !is_downloading;
                    let can_stop = has_selected && is_downloading;
                    let any_downloading = self.downloads.iter().any(|d| d.status == DownloadStatus::Downloading);
                    let has_completed = self.downloads.iter().any(|d| d.status == DownloadStatus::Complete);

                    if tool_btn(ui, "add_url", "Add URL", true) {
                        self.add_url_dialog.is_open = true;
                    }

                    if tool_btn(ui, "resume", "Resume", can_resume) {
                        if let Some(id) = self.selected_id {
                            self.resume_download(id);
                        }
                    }

                    if tool_btn(ui, "stop", "Stop", can_stop) {
                        if let Some(id) = self.selected_id {
                            self.pause_download(id);
                        }
                    }

                    if tool_btn(ui, "stop_all", "Stop All", any_downloading) {
                        self.stop_all();
                    }

                    if tool_btn(ui, "delete", "Delete", has_selected) {
                        if let Some(id) = self.selected_id {
                            self.remove_download(id);
                        }
                    }

                    if tool_btn(ui, "delete_completed", "Delete Co...", has_completed) {
                        self.remove_all_completed();
                    }

                    ui.separator();

                    if tool_btn(ui, "options", "Options", true) {
                        self.options_dialog.is_open = true;
                    }

                    if tool_btn(ui, "scheduler", "Scheduler", true) {
                        self.scheduler_dialog.is_open = true;
                    }

                    if tool_btn(ui, "start_queue", "Start Queue", true) {
                        self.queue_manager.start();
                    }

                    if tool_btn(ui, "stop_queue", "Stop Queue", true) {
                        self.queue_manager.stop();
                    }

                    if tool_btn(ui, "grabber", "Grabber", true) {
                        self.show_about = true;
                    }

                    if tool_btn(ui, "tell_friend", "Tell a Frie...", true) {
                        self.show_about = true;
                    }
                });
                ui.add_space(3.0);
            });

        // 3. Bottom Status Bar Panel with 4 sunken panels
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::none().fill(IdmTheme::STATUS_BAR_BG))
            .show(ctx, |ui| {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                    let active_count = self.downloads.iter().filter(|d| d.status == DownloadStatus::Downloading).count();
                    let total_speed: u64 = self.active_updates.values().map(|u| u.speed_bps).sum();

                    let speed_str = if total_speed >= 1_048_576 {
                        format!("{:.2} MB/s", total_speed as f64 / 1_048_576.0)
                    } else {
                        format!("{:.1} KB/s", total_speed as f64 / 1024.0)
                    };

                    let limit_str = if let Some(l) = self.config.speed_limit {
                        format!("{:.0} KB/s", l as f64 / 1024.0)
                    } else {
                        "OFF".to_string()
                    };

                    let draw_panel = |ui: &mut egui::Ui, text: &str, min_w: f32| {
                        let rect = ui.allocate_exact_size(Vec2::new(min_w.max(text.len() as f32 * 7.5 + 16.0), 18.0), Sense::hover());
                        let painter = ui.painter();
                        painter.rect_filled(rect.0, 0.0, IdmTheme::STATUS_BAR_PANEL);
                        painter.rect_stroke(rect.0, 0.0, Stroke::new(1.0_f32, IdmTheme::STATUS_BAR_BORDER));
                        painter.text(
                            egui::pos2(rect.0.left() + 6.0, rect.0.center().y),
                            egui::Align2::LEFT_CENTER,
                            text,
                            egui::FontId::proportional(11.0),
                            IdmTheme::BLACK,
                        );
                    };

                    draw_panel(ui, &format!("{} download(s)", self.downloads.len()), 110.0);
                    draw_panel(ui, &format!("Active: {}", active_count), 75.0);
                    draw_panel(ui, &format!("Total Speed: {}", speed_str), 140.0);
                    draw_panel(ui, &format!("Speed Limiter: {}", limit_str), 125.0);
                });
                ui.add_space(2.0);
            });

        // 4. Left Sidebar (Categories Tree) - Visible when show_categories is true
        if self.show_categories {
            egui::SidePanel::left("categories_panel")
                .default_width(175.0)
                .width_range(140.0..=260.0)
                .frame(egui::Frame::none().fill(IdmTheme::WHITE).stroke(Stroke::new(1.0_f32, IdmTheme::WIN_BORDER)))
                .show(ctx, |ui| {
                    // Header bar with "Categories" title and close [x] button
                    ui.horizontal(|ui| {
                        ui.add_space(4.0);
                        ui.strong("Categories");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("×").clicked() {
                                self.show_categories = false;
                            }
                        });
                    });
                    ui.separator();

                    // Categories Collapsible Root Node
                    ui.horizontal(|ui| {
                        let sym = if self.cat_all_expanded { "-" } else { "+" };
                        if ui.small_button(sym).clicked() {
                            self.cat_all_expanded = !self.cat_all_expanded;
                        }
                        let f_icon = if self.cat_all_expanded {
                            egui::include_image!("../../assets/idm_gui/tree/folder_open.png")
                        } else {
                            egui::include_image!("../../assets/idm_gui/tree/folder_closed.png")
                        };
                        ui.add(egui::Image::new(f_icon).fit_to_exact_size(Vec2::new(16.0, 13.0)));
                        ui.label(egui::RichText::new("Categories").strong());
                    });

                    if self.cat_all_expanded {
                        let cats = [
                            ("All Downloads", egui::include_image!("../../assets/idm_gui/tree/all_downloads.png")),
                            ("Compressed", egui::include_image!("../../assets/idm_gui/tree/compressed.png")),
                            ("Documents", egui::include_image!("../../assets/idm_gui/tree/documents.png")),
                            ("Music", egui::include_image!("../../assets/idm_gui/tree/music.png")),
                            ("Programs", egui::include_image!("../../assets/idm_gui/tree/programs.png")),
                            ("Video", egui::include_image!("../../assets/idm_gui/tree/video.png")),
                            ("Finished", egui::include_image!("../../assets/idm_gui/tree/finished.png")),
                            ("Unfinished", egui::include_image!("../../assets/idm_gui/tree/unfinished.png")),
                        ];

                        for (cat_name, icon_uri) in cats {
                            let count = match cat_name {
                                "All Downloads" => self.downloads.len(),
                                "Finished" => self.downloads.iter().filter(|d| d.status == DownloadStatus::Complete).count(),
                                "Unfinished" => self.downloads.iter().filter(|d| d.status != DownloadStatus::Complete).count(),
                                other => self.downloads.iter().filter(|d| d.category == other).count(),
                            };

                            let is_sel = self.selected_category == cat_name;
                            let label_text = format!("{} ({})", cat_name, count);

                            let row_resp = ui.horizontal(|ui| {
                                ui.add_space(20.0);
                                ui.add(egui::Image::new(icon_uri).fit_to_exact_size(Vec2::new(16.0, 13.0)));
                                if is_sel {
                                    ui.label(egui::RichText::new(label_text).color(IdmTheme::WHITE).background_color(IdmTheme::SELECTION_BLUE));
                                } else {
                                    ui.label(label_text);
                                }
                            });

                            if row_resp.response.interact(Sense::click()).clicked() {
                                self.selected_category = cat_name.to_string();
                            }
                        }
                    }

                    ui.add_space(10.0);

                    // Queues Collapsible Root Node
                    ui.horizontal(|ui| {
                        let sym = if self.cat_queues_expanded { "-" } else { "+" };
                        if ui.small_button(sym).clicked() {
                            self.cat_queues_expanded = !self.cat_queues_expanded;
                        }
                        let f_icon = if self.cat_queues_expanded {
                            egui::include_image!("../../assets/idm_gui/tree/folder_open.png")
                        } else {
                            egui::include_image!("../../assets/idm_gui/tree/folder_closed.png")
                        };
                        ui.add(egui::Image::new(f_icon).fit_to_exact_size(Vec2::new(16.0, 13.0)));
                        ui.label(egui::RichText::new("Queues").strong());
                    });

                    if self.cat_queues_expanded {
                        let queues = [
                            ("Main download queue", egui::include_image!("../../assets/idm_gui/tree/queue_main.png")),
                            ("Synchronization queue", egui::include_image!("../../assets/idm_gui/tree/queue_sync.png")),
                        ];

                        for (q_name, icon_uri) in queues {
                            let is_sel = self.selected_category == q_name;

                            let row_resp = ui.horizontal(|ui| {
                                ui.add_space(20.0);
                                ui.add(egui::Image::new(icon_uri).fit_to_exact_size(Vec2::new(16.0, 13.0)));
                                if is_sel {
                                    ui.label(egui::RichText::new(q_name).color(IdmTheme::WHITE).background_color(IdmTheme::SELECTION_BLUE));
                                } else {
                                    ui.label(q_name);
                                }
                            });

                            if row_resp.response.interact(Sense::click()).clicked() {
                                self.selected_category = q_name.to_string();
                            }
                        }
                    }
                });
        }

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

                let table_height = (ui.available_height() - 30.0).max(100.0);
                egui::ScrollArea::both()
                    .max_height(table_height)
                    .show(ui, |ui| {
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
                                    if item.status == DownloadStatus::Idle {
                                        let q_icon = egui::include_image!("../../assets/idm_gui/tree/folder_closed.png");
                                        ui.add(egui::Image::new(q_icon).fit_to_exact_size(Vec2::new(14.0, 12.0)));
                                    } else {
                                        ui.label("");
                                    }

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

                // Bottom URL edit box (single line, full width) showing the URL of the selected item
                ui.separator();
                let mut selected_url = self.selected_id
                    .and_then(|id| self.downloads.iter().find(|d| d.id == id))
                    .map(|d| d.url.clone())
                    .unwrap_or_default();
                ui.add_sized([ui.available_width(), 20.0], egui::TextEdit::singleline(&mut selected_url));
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
            egui::Window::new("About Internet Download Manager")
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .default_width(380.0)
                .show(ctx, |ui| {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let app_icon = egui::include_image!("../../assets/idm_gui/dialogs/group_128_48x48.png");
                        ui.add(egui::Image::new(app_icon).fit_to_exact_size(Vec2::new(48.0, 48.0)));
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Internet Download Manager").strong().size(15.0));
                            ui.label("Version 6.43 (Rust Re-Implementation)");
                            ui.label("High-Performance Dynamic In-Half Segmentation");
                        });
                    });

                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.set_width(ui.available_width());
                        ui.label(egui::RichText::new("This product is licensed to:").strong());
                        ui.label("ReIDM Community / Free & Open Source");
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("Home Site:");
                        ui.hyperlink("http://www.internetdownloadmanager.com");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Support Team:");
                        ui.hyperlink("http://www.internetdownloadmanager.com/contact_us.html");
                    });

                    ui.add_space(6.0);
                    ui.small("Copyright © 1999-2026 Tonec FZE. All rights reserved.");
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([75.0, 24.0], egui::Button::new("Close")).clicked() {
                            self.show_about = false;
                        }
                    });
                });
        }

        // Request continuous repaint while downloads are active for smooth 60 FPS progress updates
        let any_downloading = self.downloads.iter().any(|d| d.status == DownloadStatus::Downloading);
        if any_downloading || self.progress_dialog.is_open {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
