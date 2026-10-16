use crate::types::DownloadItem;
use eframe::egui::{self, Align2, Button, Vec2, Window};

pub struct DownloadCompleteDialog {
    pub is_open: bool,
    pub item: Option<DownloadItem>,
}

impl Default for DownloadCompleteDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            item: None,
        }
    }
}

pub enum CompleteDialogAction {
    Open,
    OpenFolder,
    Close,
}

impl DownloadCompleteDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Option<CompleteDialogAction> {
        if !self.is_open {
            return None;
        }

        let item = self.item.as_ref()?;
        let mut action = None;

        Window::new("Download complete")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("📁");
                    ui.vertical(|ui| {
                        ui.strong(&item.file_name);
                        ui.label(format!("Size: {:.2} MB", item.file_size as f64 / 1_048_576.0));
                        ui.label(format!("Saved to: {}", item.final_destination().display()));
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([75.0, 24.0], Button::new("Close")).clicked() {
                            self.is_open = false;
                            action = Some(CompleteDialogAction::Close);
                        }

                        if ui.add_sized([90.0, 24.0], Button::new("Open folder")).clicked() {
                            action = Some(CompleteDialogAction::OpenFolder);
                        }

                        if ui.add_sized([75.0, 24.0], Button::new("Open")).clicked() {
                            action = Some(CompleteDialogAction::Open);
                        }
                    });
                });
            });

        action
    }
}
