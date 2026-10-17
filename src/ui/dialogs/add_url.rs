use eframe::egui::{self, Align2, Button, TextEdit, Vec2, Window};

pub struct AddUrlDialog {
    pub is_open: bool,
    pub url_input: String,
    pub use_auth: bool,
    pub login_input: String,
    pub password_input: String,
}

impl Default for AddUrlDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            url_input: String::new(),
            use_auth: false,
            login_input: String::new(),
            password_input: String::new(),
        }
    }
}

impl AddUrlDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Option<String> {
        if !self.is_open {
            return None;
        }

        let mut confirmed_url = None;

        Window::new("Enter new URL to download")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    // Left main section
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("URL: ");
                            ui.add_sized(
                                [320.0, 22.0],
                                TextEdit::singleline(&mut self.url_input)
                                    .hint_text("http:// or https:// URL"),
                            );
                        });

                        ui.add_space(6.0);
                        ui.group(|ui| {
                            ui.set_width(360.0);
                            ui.checkbox(&mut self.use_auth, "Use authorization");
                            if self.use_auth {
                                ui.horizontal(|ui| {
                                    ui.label("Login:   ");
                                    ui.add_sized([95.0, 20.0], TextEdit::singleline(&mut self.login_input));
                                    ui.label("Password:");
                                    ui.add_sized([95.0, 20.0], TextEdit::singleline(&mut self.password_input).password(true));
                                });
                            }
                        });
                    });

                    ui.add_space(8.0);

                    // Right button column
                    ui.vertical(|ui| {
                        let can_submit = !self.url_input.trim().is_empty();
                        let ok_btn = ui.add_enabled_ui(can_submit, |ui| {
                            ui.add_sized([78.0, 24.0], Button::new("OK"))
                        });

                        if ok_btn.inner.clicked() || (ui.input(|i| i.key_pressed(egui::Key::Enter)) && can_submit) {
                            confirmed_url = Some(self.url_input.trim().to_string());
                            self.is_open = false;
                        }

                        ui.add_space(6.0);
                        if ui.add_sized([78.0, 24.0], Button::new("Cancel")).clicked() {
                            self.is_open = false;
                        }
                    });
                });
            });

        confirmed_url
    }
}
