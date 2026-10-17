use crate::config::Config;
use eframe::egui::{self, Align2, Button, ComboBox, TextEdit, Vec2, Window};
use std::path::PathBuf;

#[derive(PartialEq)]
pub enum OptionsTab {
    General,
    Connection,
    SaveTo,
}

pub struct OptionsDialog {
    pub is_open: bool,
    pub active_tab: OptionsTab,
    pub max_connections: usize,
    pub speed_limit_kb: u64,
    pub download_dir: PathBuf,
    pub temp_dir: PathBuf,
}

impl Default for OptionsDialog {
    fn default() -> Self {
        let config = Config::default();
        Self {
            is_open: false,
            active_tab: OptionsTab::Connection,
            max_connections: config.max_connections,
            speed_limit_kb: config.speed_limit.unwrap_or(0) / 1024,
            download_dir: config.download_dir,
            temp_dir: config.temp_dir,
        }
    }
}

pub enum OptionsAction {
    Save(Config),
    Cancel,
}

impl OptionsDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Option<OptionsAction> {
        if !self.is_open {
            return None;
        }

        let mut action = None;

        Window::new("Internet Download Manager Configuration")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(self.active_tab == OptionsTab::Connection, "Connection").clicked() {
                        self.active_tab = OptionsTab::Connection;
                    }
                    if ui.selectable_label(self.active_tab == OptionsTab::SaveTo, "Save To").clicked() {
                        self.active_tab = OptionsTab::SaveTo;
                    }
                    if ui.selectable_label(self.active_tab == OptionsTab::General, "General").clicked() {
                        self.active_tab = OptionsTab::General;
                    }
                });
                ui.separator();
                ui.add_space(8.0);

                match self.active_tab {
                    OptionsTab::Connection => {
                        ui.label("Max. connections number for a server:");
                        ui.horizontal(|ui| {
                            ComboBox::from_id_salt("max_conns_combo")
                                .selected_text(format!("{} connections", self.max_connections))
                                .show_ui(ui, |ui| {
                                    for count in [1, 2, 4, 8, 16, 24, 32] {
                                        ui.selectable_value(&mut self.max_connections, count, format!("{} connections", count));
                                    }
                                });
                        });

                        ui.add_space(10.0);
                        ui.label("Speed Limiter:");
                        ui.horizontal(|ui| {
                            ui.label("Default speed limit:");
                            let mut limit_str = self.speed_limit_kb.to_string();
                            if ui.add_sized([80.0, 20.0], TextEdit::singleline(&mut limit_str)).changed() {
                                if let Ok(val) = limit_str.parse::<u64>() {
                                    self.speed_limit_kb = val;
                                }
                            }
                            ui.label("KB/s (0 = unlimited)");
                        });
                    }
                    OptionsTab::SaveTo => {
                        ui.label("Default download directory:");
                        let mut dl_str = self.download_dir.to_string_lossy().to_string();
                        if ui.add_sized([ui.available_width() - 8.0, 20.0], TextEdit::singleline(&mut dl_str)).changed() {
                            self.download_dir = PathBuf::from(dl_str);
                        }

                        ui.add_space(8.0);
                        ui.label("Temporary directory for chunk assembly:");
                        let mut temp_str = self.temp_dir.to_string_lossy().to_string();
                        if ui.add_sized([ui.available_width() - 8.0, 20.0], TextEdit::singleline(&mut temp_str)).changed() {
                            self.temp_dir = PathBuf::from(temp_str);
                        }
                    }
                    OptionsTab::General => {
                        ui.label("Browser Integration:");
                        ui.label("Native Messaging Host: com.tonec.idm (Active)");
                        ui.label("Extension WebSocket Server: ws://127.0.0.1:1001 (Active)");
                        ui.add_space(8.0);
                        if ui.button("Install / Update Browser Integration Manifests").clicked() {
                            // install manifests
                        }
                    }
                }

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([75.0, 24.0], Button::new("Cancel")).clicked() {
                            self.is_open = false;
                            action = Some(OptionsAction::Cancel);
                        }

                        if ui.add_sized([75.0, 24.0], Button::new("OK")).clicked() {
                            self.is_open = false;
                            let mut cfg = Config::default();
                            cfg.max_connections = self.max_connections;
                            cfg.speed_limit = if self.speed_limit_kb > 0 { Some(self.speed_limit_kb * 1024) } else { None };
                            cfg.download_dir = self.download_dir.clone();
                            cfg.temp_dir = self.temp_dir.clone();
                            action = Some(OptionsAction::Save(cfg));
                        }
                    });
                });
            });

        action
    }
}
