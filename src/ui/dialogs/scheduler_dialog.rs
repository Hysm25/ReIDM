use eframe::egui::{self, Align2, Button, Checkbox, Vec2, Window};

pub struct SchedulerDialog {
    pub is_open: bool,
    pub start_time_enabled: bool,
    pub start_hour: u32,
    pub start_minute: u32,
    pub stop_time_enabled: bool,
    pub stop_hour: u32,
    pub stop_minute: u32,
}

impl Default for SchedulerDialog {
    fn default() -> Self {
        Self {
            is_open: false,
            start_time_enabled: false,
            start_hour: 2,
            start_minute: 0,
            stop_time_enabled: false,
            stop_hour: 6,
            stop_minute: 0,
        }
    }
}

pub enum SchedulerAction {
    StartQueue,
    StopQueue,
    Close,
}

impl SchedulerDialog {
    pub fn show(&mut self, ctx: &egui::Context) -> Option<SchedulerAction> {
        if !self.is_open {
            return None;
        }

        let mut action = None;

        Window::new("Scheduler")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Left queue list
                    ui.group(|ui| {
                        ui.set_width(140.0);
                        ui.label("Queues:");
                        let _ = ui.selectable_label(true, "Main download queue");
                        let _ = ui.selectable_label(false, "Synchronization queue");
                    });

                    // Right scheduler settings
                    ui.vertical(|ui| {
                        ui.add(Checkbox::new(&mut self.start_time_enabled, "Start download at:"));
                        ui.horizontal(|ui| {
                            ui.add_enabled(self.start_time_enabled, egui::DragValue::new(&mut self.start_hour).range(0..=23));
                            ui.label(":");
                            ui.add_enabled(self.start_time_enabled, egui::DragValue::new(&mut self.start_minute).range(0..=59));
                        });

                        ui.add_space(8.0);
                        ui.add(Checkbox::new(&mut self.stop_time_enabled, "Stop download at:"));
                        ui.horizontal(|ui| {
                            ui.add_enabled(self.stop_time_enabled, egui::DragValue::new(&mut self.stop_hour).range(0..=23));
                            ui.label(":");
                            ui.add_enabled(self.stop_time_enabled, egui::DragValue::new(&mut self.stop_minute).range(0..=59));
                        });
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    if ui.add_sized([80.0, 24.0], Button::new("Start Now")).clicked() {
                        action = Some(SchedulerAction::StartQueue);
                    }
                    if ui.add_sized([80.0, 24.0], Button::new("Stop")).clicked() {
                        action = Some(SchedulerAction::StopQueue);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([75.0, 24.0], Button::new("Close")).clicked() {
                            self.is_open = false;
                            action = Some(SchedulerAction::Close);
                        }
                    });
                });
            });

        action
    }
}
