use eframe::egui::{self, Align2, Button, ComboBox, TextEdit, Vec2, Window};
use std::path::PathBuf;

pub struct DownloadInfoDialog {
    pub is_open: bool,
    pub url: String,
    pub file_name: String,
    pub save_path: PathBuf,
    pub category: String,
    pub file_size: u64,
    pub resumable: bool,
    pub description: String,
    pub remember_path: bool,
}

impl Default for DownloadInfoDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            url: String::new(),
            file_name: String::new(),
            save_path: PathBuf::from("Downloads"),
            category: "General".to_string(),
            file_size: 0,
            resumable: true,
            description: String::new(),
            remember_path: true,
        }
    }
}

pub enum DownloadInfoAction {
    StartDownload,
    DownloadLater,
    Cancel,
}

impl DownloadInfoDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Option<DownloadInfoAction> {
        if !self.is_open {
            return None;
        }

        let mut action = None;

        Window::new("Download File Info")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);

                // URL
                ui.horizontal(|ui| {
                    ui.label("URL:       ");
                    ui.add_sized(
                        [ui.available_width() - 8.0, 20.0],
                        TextEdit::singleline(&mut self.url),
                    );
                });

                ui.add_space(6.0);

                // Category
                ui.horizontal(|ui| {
                    ui.label("Category:  ");
                    ComboBox::from_id_salt("category_combo")
                        .selected_text(&self.category)
                        .show_ui(ui, |ui| {
                            for cat in &["General", "Compressed", "Documents", "Music", "Programs", "Video"] {
                                ui.selectable_value(&mut self.category, cat.to_string(), *cat);
                            }
                        });
                    let _ = ui.button("+");
                });

                ui.add_space(6.0);

                // Save As
                ui.horizontal(|ui| {
                    ui.label("Save As:    ");
                    let mut path_str = self.save_path.to_string_lossy().to_string();
                    ui.add_sized([ui.available_width() - 40.0, 20.0], TextEdit::singleline(&mut path_str));
                    self.save_path = PathBuf::from(path_str);
                    let _ = ui.button("...");
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(62.0);
                    let remember_label = format!("Remember this path for \"{}\" category", self.category);
                    ui.checkbox(&mut self.remember_path, remember_label);
                });

                ui.add_space(6.0);

                // File Size & Resumability info
                ui.horizontal(|ui| {
                    let size_str = if self.file_size > 0 {
                        format!("{:.2} MB ({} bytes)", self.file_size as f64 / 1_048_576.0, self.file_size)
                    } else {
                        "Unknown (streaming)".to_string()
                    };
                    ui.label(format!("File Size: {}", size_str));
                    ui.add_space(20.0);
                    let res_str = if self.resumable { "Yes (Multi-stream)" } else { "No (Single stream)" };
                    ui.label(format!("Resumable: {}", res_str));
                });

                ui.add_space(6.0);

                // Description
                ui.horizontal(|ui| {
                    ui.label("Description:");
                    ui.add_sized(
                        [ui.available_width() - 8.0, 20.0],
                        TextEdit::singleline(&mut self.description),
                    );
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                // Bottom Buttons matching Dialog 130: Download Later (1004) | Start Download (1006) | Cancel (2)
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([75.0, 26.0], Button::new("Cancel")).clicked() {
                            self.is_open = false;
                            action = Some(DownloadInfoAction::Cancel);
                        }

                        if ui.add_sized([120.0, 26.0], Button::new("Start Download")).clicked() {
                            self.is_open = false;
                            action = Some(DownloadInfoAction::StartDownload);
                        }

                        if ui.add_sized([115.0, 26.0], Button::new("Download Later")).clicked() {
                            self.is_open = false;
                            action = Some(DownloadInfoAction::DownloadLater);
                        }
                    });
                });
            });

        action
    }
}
