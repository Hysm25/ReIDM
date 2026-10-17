use crate::types::DownloadItem;
use eframe::egui::{self, Align2, Button, Vec2, Window};

pub struct DownloadCompleteDialog {
    pub is_open: bool,
    pub item: Option<DownloadItem>,
    pub dont_show_again: bool,
}

impl Default for DownloadCompleteDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            item: None,
            dont_show_again: false,
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
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let icon = egui::include_image!("../../../assets/idm_gui/dialogs/group_276_32x32.png");
                    ui.add(egui::Image::new(icon).fit_to_exact_size(Vec2::new(32.0, 32.0)));
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(format!(
                            "Downloaded {:.2} MB ({} Bytes)",
                            item.file_size as f64 / 1_048_576.0,
                            item.file_size
                        ));
                    });
                });

                ui.add_space(8.0);
                ui.label("Address");
                let mut url = item.url.clone();
                ui.add_sized([ui.available_width(), 20.0], egui::TextEdit::singleline(&mut url));

                ui.add_space(4.0);
                ui.label("The file saved as");
                let mut saved_as = item.final_destination().display().to_string();
                ui.add_sized([ui.available_width(), 20.0], egui::TextEdit::singleline(&mut saved_as));

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui.add_sized([75.0, 24.0], Button::new("Open")).clicked() {
                        action = Some(CompleteDialogAction::Open);
                    }
                    if ui.add_sized([85.0, 24.0], Button::new("Open with...")).clicked() {
                        action = Some(CompleteDialogAction::Open);
                    }
                    if ui.add_sized([85.0, 24.0], Button::new("Open folder")).clicked() {
                        action = Some(CompleteDialogAction::OpenFolder);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([70.0, 24.0], Button::new("Close")).clicked() {
                            self.is_open = false;
                            action = Some(CompleteDialogAction::Close);
                        }
                    });
                });

                ui.add_space(4.0);
                ui.checkbox(&mut self.dont_show_again, "Don't show this dialog again");
            });

        action
    }
}
