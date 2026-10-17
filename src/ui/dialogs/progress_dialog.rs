use crate::engine::segmenter::{ProgressUpdate, SegmentProgress};
use crate::types::{DownloadItem, DownloadStatus};
use crate::ui::segment_widget::SegmentVisualizer;
use crate::ui::theme::IdmTheme;
use eframe::egui::{self, Align2, Button, Checkbox, ProgressBar, TextEdit, Vec2, Window};

#[derive(PartialEq)]
pub enum ProgressTab {
    Status,
    Options,
    SpeedLimiter,
}

pub struct ProgressDialog {
    pub is_open: bool,
    pub download_id: u64,
    pub active_tab: ProgressTab,
    pub show_details: bool,
    pub turn_off_pc: bool,
    pub exit_on_done: bool,
    pub speed_limiter_enabled: bool,
    pub speed_limit_kb: u64,
}

impl Default for ProgressDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            download_id: 0,
            active_tab: ProgressTab::Status,
            show_details: true,
            turn_off_pc: false,
            exit_on_done: false,
            speed_limiter_enabled: false,
            speed_limit_kb: 500,
        }
    }
}

pub enum ProgressDialogAction {
    Pause,
    Resume,
    Cancel,
}

impl ProgressDialog {
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        item: &DownloadItem,
        progress: Option<&ProgressUpdate>,
    ) -> Option<ProgressDialogAction> {
        if !self.is_open {
            return None;
        }

        let mut action = None;

        let total_bytes = if let Some(p) = progress {
            p.total_bytes
        } else {
            item.file_size
        };

        let downloaded_bytes = if let Some(p) = progress {
            p.downloaded_bytes
        } else {
            item.downloaded
        };

        let speed = if let Some(p) = progress {
            p.speed_bps
        } else {
            0
        };

        let active_conns = if let Some(p) = progress {
            p.active_connections
        } else {
            0
        };

        let segments: Vec<SegmentProgress> = if let Some(p) = progress {
            p.segments.clone()
        } else {
            item.segments
                .iter()
                .map(|s| SegmentProgress {
                    id: s.id,
                    start_pos: s.start_pos,
                    current_pos: s.current_pos,
                    end_pos: s.end_pos,
                    downloaded: s.downloaded,
                    is_active: s.is_active,
                })
                .collect()
        };

        let percent = if total_bytes > 0 {
            (downloaded_bytes as f64 / total_bytes as f64 * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };

        let window_title = format!("{:.1}% of {}", percent, item.file_name);

        Window::new(window_title)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(500.0)
            .show(ctx, |ui| {
                // Tab bar (Download status | Options on completion | Speed Limiter)
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.active_tab == ProgressTab::Status, "Download status").clicked() {
                        self.active_tab = ProgressTab::Status;
                    }
                    if ui.selectable_label(self.active_tab == ProgressTab::Options, "Options on completion").clicked() {
                        self.active_tab = ProgressTab::Options;
                    }
                    if ui.selectable_label(self.active_tab == ProgressTab::SpeedLimiter, "Speed Limiter").clicked() {
                        self.active_tab = ProgressTab::SpeedLimiter;
                    }
                });
                ui.separator();

                match self.active_tab {
                    ProgressTab::Status => {
                        egui::Grid::new("status_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                            ui.label("Status:");
                            let status_text = match item.status {
                                DownloadStatus::Downloading => format!("Downloading ({} active connections)", active_conns),
                                DownloadStatus::Paused => "Paused".to_string(),
                                DownloadStatus::Complete => "Complete".to_string(),
                                DownloadStatus::Connecting => "Connecting...".to_string(),
                                DownloadStatus::Error => "Error".to_string(),
                                DownloadStatus::Idle => "Queued".to_string(),
                            };
                            ui.colored_label(IdmTheme::STATUS_DOWNLOADING, status_text);
                            ui.end_row();

                            ui.label("File size:");
                            ui.label(if total_bytes > 0 {
                                format!("{:.2} MB ({} bytes)", total_bytes as f64 / 1_048_576.0, total_bytes)
                            } else {
                                "Unknown (streaming)".to_string()
                            });
                            ui.end_row();

                            ui.label("Downloaded:");
                            ui.label(format!("{:.2} MB ({:.2}%)", downloaded_bytes as f64 / 1_048_576.0, percent));
                            ui.end_row();

                            ui.label("Transfer rate:");
                            let speed_str = if speed >= 1_048_576 {
                                format!("{:.2} MB/s", speed as f64 / 1_048_576.0)
                            } else {
                                format!("{:.1} KB/s", speed as f64 / 1024.0)
                            };
                            ui.colored_label(IdmTheme::STATUS_COMPLETE, speed_str);
                            ui.end_row();

                            ui.label("Time left:");
                            let time_left = if speed > 0 && total_bytes > downloaded_bytes {
                                let secs = (total_bytes - downloaded_bytes) / speed;
                                format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
                            } else if item.status == DownloadStatus::Complete {
                                "00:00:00".to_string()
                            } else {
                                "--:--:--".to_string()
                            };
                            ui.label(time_left);
                            ui.end_row();

                            ui.label("Resume capability:");
                            ui.label(if item.resumable { "Yes" } else { "No" });
                            ui.end_row();
                        });
                    }
                    ProgressTab::Options => {
                        ui.add_space(8.0);
                        ui.add(Checkbox::new(&mut self.turn_off_pc, "Turn off computer when done"));
                        ui.add(Checkbox::new(&mut self.exit_on_done, "Exit ReIDM when download is finished"));
                    }
                    ProgressTab::SpeedLimiter => {
                        ui.add_space(8.0);
                        ui.add(Checkbox::new(&mut self.speed_limiter_enabled, "Use Speed Limiter"));
                        ui.horizontal(|ui| {
                            ui.label("Maximum download speed:");
                            let mut s = self.speed_limit_kb.to_string();
                            if ui.add_sized([80.0, 20.0], TextEdit::singleline(&mut s)).changed() {
                                if let Ok(val) = s.parse::<u64>() {
                                    self.speed_limit_kb = val;
                                }
                            }
                            ui.label("KBytes/sec");
                        });
                    }
                }

                ui.add_space(8.0);

                // Standard linear progress bar
                let progress_fraction = (percent / 100.0) as f32;
                ui.add(
                    ProgressBar::new(progress_fraction)
                        .show_percentage()
                        .animate(item.status == DownloadStatus::Downloading),
                );

                ui.add_space(6.0);

                // Dynamic Segmented Progress Visualizer (The Signature IDM Bar)
                ui.label("Start positions and download progress by connections:");
                ui.add(SegmentVisualizer::new(total_bytes, &segments).height(22.0));

                ui.add_space(4.0);

                // Connection details table (SysListView32 in IDM)
                if self.show_details {
                    ui.group(|ui| {
                        egui::ScrollArea::vertical().max_height(90.0).show(ui, |ui| {
                            egui::Grid::new("conn_table")
                                .striped(true)
                                .min_col_width(70.0)
                                .show(ui, |ui| {
                                    ui.strong("N.");
                                    ui.strong("Downloaded");
                                    ui.strong("Info");
                                    ui.end_row();

                                    for seg in &segments {
                                        ui.label(format!("{}", seg.id));
                                        ui.label(format!("{} KB", seg.downloaded / 1024));
                                        let info = if seg.is_active {
                                            "Receiving data..."
                                        } else if seg.current_pos > seg.end_pos {
                                            "Complete"
                                        } else {
                                            "Connecting..."
                                        };
                                        ui.label(info);
                                        ui.end_row();
                                    }
                                });
                        });
                    });
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);

                // Action buttons: Details | Pause / Resume | Cancel
                ui.horizontal(|ui| {
                    let details_label = if self.show_details { "<< Hide details" } else { ">> Show details" };
                    if ui.add_sized([105.0, 26.0], Button::new(details_label)).clicked() {
                        self.show_details = !self.show_details;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([75.0, 26.0], Button::new("Cancel")).clicked() {
                            self.is_open = false;
                            action = Some(ProgressDialogAction::Cancel);
                        }

                        if item.status == DownloadStatus::Downloading {
                            if ui.add_sized([75.0, 26.0], Button::new("Pause")).clicked() {
                                action = Some(ProgressDialogAction::Pause);
                            }
                        } else if item.status == DownloadStatus::Paused {
                            if ui.add_sized([75.0, 26.0], Button::new("Resume")).clicked() {
                                action = Some(ProgressDialogAction::Resume);
                            }
                        }
                    });
                });
            });

        action
    }
}
