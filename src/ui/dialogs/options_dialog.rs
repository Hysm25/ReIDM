use crate::config::Config;
use eframe::egui::{self, Align2, Button, Checkbox, ComboBox, TextEdit, Vec2, Window};
use std::path::PathBuf;

/// Tab names matching the original IDM Configuration dialog tab bar
/// Verified from screenshots: General | File types | Save to | Downloads | Connection | Proxy | Sites Logins | Dial Up | Sounds
#[derive(PartialEq, Clone, Copy)]
pub enum OptionsTab {
    General,
    FileTypes,
    SaveTo,
    Downloads,
    Connection,
    Proxy,
    SitesLogins,
    DialUp,
    Sounds,
}

pub struct OptionsDialog {
    pub is_open: bool,
    pub active_tab: OptionsTab,
    // Connection tab
    pub max_connections: usize,
    pub connection_type: String,
    pub connection_speed: String,
    // Save To tab
    pub download_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub category: String,
    pub category_extensions: String,
    pub remember_last_path: bool,
    // General tab
    pub launch_on_startup: bool,
    pub monitor_clipboard: bool,
    pub use_advanced_integration: bool,
    // File Types tab
    pub file_types: String,
    pub exception_sites: String,
    pub exception_addresses: String,
    // Downloads tab
    pub show_start_dialog: bool,
    pub show_complete_dialog: bool,
    pub progress_dialog_mode: String,
    pub start_immediately: bool,
    pub show_queue_on_later: bool,
    pub show_queue_on_batch_close: bool,
    pub ignore_modification_time: bool,
    pub duplicate_action: String,
    pub virus_scanner: String,
    pub virus_params: String,
    // Speed limiter
    pub speed_limit_kb: u64,
    pub download_limit_mb: u64,
    pub download_limit_hours: u32,
    pub show_limit_warning: bool,
    // Sounds tab
    pub sound_complete: bool,
    pub sound_failed: bool,
    pub sound_queue_started: bool,
    pub sound_queue_finished: bool,
}

impl Default for OptionsDialog {
    fn default() -> Self {
        let config = Config::default();
        Self {
            is_open: false,
            active_tab: OptionsTab::General,
            max_connections: config.max_connections,
            connection_type: "Other".to_string(),
            connection_speed: "12800000".to_string(),
            download_dir: config.download_dir,
            temp_dir: config.temp_dir,
            category: "Video".to_string(),
            category_extensions: "avi mp4 mkv webm flv mov wmv".to_string(),
            remember_last_path: true,
            launch_on_startup: false,
            monitor_clipboard: false,
            use_advanced_integration: false,
            file_types: "EXE ZIP APU RAR LZH Z GZ GZIP TAR BIN MP3 M4A WAV RA RAM AAC AIF AVI MPG MPEG QT PLJ ASF MOV RM MP4 WMA WMV MKE MPP RTF AT1 TIF TIFF PDF MIS AGE ISO IMG DGB 7Z SEA SIT SITX PPT PPS".to_string(),
            exception_sites: "*.update.microsoft.com download.windowsupdate.com *.real.com *.ihave.com".to_string(),
            exception_addresses: String::new(),
            show_start_dialog: true,
            show_complete_dialog: true,
            progress_dialog_mode: "Show normally sized".to_string(),
            start_immediately: true,
            show_queue_on_later: true,
            show_queue_on_batch_close: true,
            ignore_modification_time: false,
            duplicate_action: "Add the duplicate with a numbered file name".to_string(),
            virus_scanner: String::new(),
            virus_params: String::new(),
            speed_limit_kb: config.speed_limit.unwrap_or(0) / 1024,
            download_limit_mb: 200,
            download_limit_hours: 2,
            show_limit_warning: true,
            sound_complete: true,
            sound_failed: true,
            sound_queue_started: false,
            sound_queue_finished: false,
        }
    }
}

pub enum OptionsAction {
    Save(Config),
    #[allow(dead_code)]
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
            .default_width(480.0)
            .show(ctx, |ui| {
                // Tab bar matching original IDM: two rows of tabs
                // Row 1: General | File types | Save to | Downloads | Connection
                // Row 2: Proxy | Sites Logins | Dial Up | Sounds
                ui.horizontal_wrapped(|ui| {
                    let tabs_row1 = [
                        (OptionsTab::General, "General"),
                        (OptionsTab::FileTypes, "File types"),
                        (OptionsTab::SaveTo, "Save to"),
                        (OptionsTab::Downloads, "Downloads"),
                        (OptionsTab::Connection, "Connection"),
                    ];
                    for (tab, label) in tabs_row1 {
                        if ui.selectable_label(self.active_tab == tab, label).clicked() {
                            self.active_tab = tab;
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let tabs_row2 = [
                        (OptionsTab::Proxy, "Proxy"),
                        (OptionsTab::SitesLogins, "Sites Logins"),
                        (OptionsTab::DialUp, "Dial Up"),
                        (OptionsTab::Sounds, "Sounds"),
                    ];
                    for (tab, label) in tabs_row2 {
                        if ui.selectable_label(self.active_tab == tab, label).clicked() {
                            self.active_tab = tab;
                        }
                    }
                });
                ui.separator();
                ui.add_space(6.0);

                match self.active_tab {
                    OptionsTab::General => {
                        // Verified from general1.png
                        ui.heading("Browser/System Integration");
                        ui.add_space(4.0);
                        ui.add(Checkbox::new(&mut self.launch_on_startup, "Launch Internet Download Manager on startup"));
                        ui.add(Checkbox::new(&mut self.monitor_clipboard, "Automatically start downloading of URLs placed to clipboard"));
                        ui.add(Checkbox::new(&mut self.use_advanced_integration, "Use advanced browser integration"));

                        ui.add_space(8.0);
                        ui.group(|ui| {
                            ui.label("Integrate IDM into browsers:");
                            ui.add_space(2.0);
                            let browsers = [
                                "Google Chrome", "Mozilla Firefox", "Microsoft Edge",
                                "Brave Browser", "Opera", "Vivaldi",
                            ];
                            for b in browsers {
                                let mut checked = true;
                                ui.add(Checkbox::new(&mut checked, b));
                            }
                        });

                        ui.add_space(8.0);
                        let mut detect = true;
                        ui.add(Checkbox::new(&mut detect, "Detect new applications that try to download files from the Internet"));
                    }
                    OptionsTab::FileTypes => {
                        // Verified from FileTypes.png
                        ui.heading("Downloaded file types");
                        ui.add_space(4.0);
                        ui.label("Automatically start downloading the following file types:");
                        ui.add_sized(
                            [ui.available_width() - 8.0, 60.0],
                            TextEdit::multiline(&mut self.file_types),
                        );
                        ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("Default").clicked() {
                                    self.file_types = "EXE ZIP APU RAR LZH Z GZ GZIP TAR BIN MP3 M4A WAV RA RAM AAC AIF AVI MPG MPEG QT PLJ ASF MOV RM MP4 WMA WMV MKE MPP RTF AT1 TIF TIFF PDF MIS AGE ISO IMG DGB 7Z SEA SIT SITX PPT PPS".to_string();
                                }
                            });
                        });

                        ui.add_space(6.0);
                        ui.label("Don't start downloading automatically from the following sites:");
                        ui.add_sized(
                            [ui.available_width() - 8.0, 40.0],
                            TextEdit::multiline(&mut self.exception_sites),
                        );
                        ui.label("(separate names by spaces)");
                        ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("Default").clicked() {
                                    self.exception_sites = "*.update.microsoft.com download.windowsupdate.com *.real.com *.ihave.com".to_string();
                                }
                            });
                        });

                        ui.add_space(6.0);
                        ui.label("Don't start downloading automatically from the following addresses:");
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                [ui.available_width() - 80.0, 20.0],
                                TextEdit::singleline(&mut self.exception_addresses),
                            );
                            let _ = ui.button("Edit list...");
                        });
                    }
                    OptionsTab::SaveTo => {
                        // Verified from SaveTo.png
                        ui.heading("Categories, file types, folders");
                        ui.add_space(4.0);
                        ui.label("Save To...");
                        ui.horizontal(|ui| {
                            ui.label("Category");
                            ComboBox::from_id_salt("save_cat_combo")
                                .selected_text(&self.category)
                                .show_ui(ui, |ui| {
                                    for cat in &["General", "Compressed", "Documents", "Music", "Programs", "Video"] {
                                        ui.selectable_value(&mut self.category, cat.to_string(), *cat);
                                    }
                                });
                        });

                        ui.add_space(4.0);
                        ui.label(format!("Automatically put in \"{}\" category the following file types:", self.category));
                        ui.add_sized(
                            [ui.available_width() - 8.0, 20.0],
                            TextEdit::singleline(&mut self.category_extensions),
                        );

                        ui.add_space(4.0);
                        ui.label(format!("Default download directory for \"{}\" category:", self.category));
                        let mut dl_str = self.download_dir.to_string_lossy().to_string();
                        ui.horizontal(|ui| {
                            if ui.add_sized([ui.available_width() - 80.0, 20.0], TextEdit::singleline(&mut dl_str)).changed() {
                                self.download_dir = PathBuf::from(dl_str);
                            }
                            let _ = ui.button("Browse");
                        });

                        ui.add(Checkbox::new(&mut self.remember_last_path, format!("Remember last save path for \"{}\" category", self.category)));

                        ui.add_space(12.0);
                        ui.label("Temporary directory");
                        let mut temp_str = self.temp_dir.to_string_lossy().to_string();
                        ui.horizontal(|ui| {
                            if ui.add_sized([ui.available_width() - 80.0, 20.0], TextEdit::singleline(&mut temp_str)).changed() {
                                self.temp_dir = PathBuf::from(temp_str);
                            }
                            let _ = ui.button("Browse");
                        });

                        ui.add_space(4.0);
                        ui.small("Temporary directory is required for storing file parts during download.");
                        ui.small("If you have several physical drives on your computer, you should select");
                        ui.small("different physical drives for temporary directory and \"Save To\" folders for");
                        ui.small("faster assembling of downloaded files.");
                    }
                    OptionsTab::Downloads => {
                        // Verified from Downloads.png
                        ui.heading("Default download settings");
                        ui.add_space(4.0);

                        ui.label("Dialogs:");
                        ui.horizontal(|ui| {
                            ui.label("Download progress dialog");
                            ComboBox::from_id_salt("progress_mode_combo")
                                .selected_text(&self.progress_dialog_mode)
                                .show_ui(ui, |ui| {
                                    for mode in &["Show normally sized", "Show minimized", "Don't show"] {
                                        ui.selectable_value(&mut self.progress_dialog_mode, mode.to_string(), *mode);
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.add(Checkbox::new(&mut self.show_start_dialog, "Show start download dialog"));
                            ui.add(Checkbox::new(&mut self.show_complete_dialog, "Show download complete dialog"));
                        });
                        ui.small("Note: These settings don't relate to queue processing");

                        ui.add_space(6.0);
                        ui.add(Checkbox::new(&mut self.start_immediately, "Start downloading immediately while displaying \"Download File Info\" dialog"));
                        ui.add(Checkbox::new(&mut self.show_queue_on_later, "Show queue selection panel on pressing \"Download Later\" button"));
                        ui.add(Checkbox::new(&mut self.show_queue_on_batch_close, "Show queue selection panel on closing batch download dialogs"));
                        ui.add_space(4.0);
                        ui.add(Checkbox::new(&mut self.ignore_modification_time, "Ignore file modification time changes when resuming a download"));

                        ui.add_space(8.0);
                        ui.group(|ui| {
                            ui.label("Virus checking");
                            ui.horizontal(|ui| {
                                ui.label("Virus scanner program");
                                ui.add_sized([180.0, 20.0], TextEdit::singleline(&mut self.virus_scanner));
                                let _ = ui.button("Browse");
                            });
                            ui.horizontal(|ui| {
                                ui.label("Command line parameters");
                                ui.add_sized([180.0, 20.0], TextEdit::singleline(&mut self.virus_params));
                            });
                        });

                        ui.add_space(4.0);
                        ui.label("If a duplicate download link is added:");
                        ComboBox::from_id_salt("duplicate_combo")
                            .selected_text(&self.duplicate_action)
                            .width(300.0)
                            .show_ui(ui, |ui| {
                                for act in &["Add the duplicate with a numbered file name", "Don't add the duplicate", "Ask the user"] {
                                    ui.selectable_value(&mut self.duplicate_action, act.to_string(), *act);
                                }
                            });
                    }
                    OptionsTab::Connection => {
                        // Verified from connection.png
                        ui.heading("Connection type");
                        ui.add_space(4.0);
                        ui.label("Connection Type/Speed");
                        ui.horizontal(|ui| {
                            ComboBox::from_id_salt("conn_type_combo")
                                .selected_text(&self.connection_type)
                                .show_ui(ui, |ui| {
                                    for t in &["Other", "Low speed (dial-up)", "Medium speed (DSL/Cable)", "High speed (LAN)"] {
                                        ui.selectable_value(&mut self.connection_type, t.to_string(), *t);
                                    }
                                });
                            ui.add_sized([100.0, 20.0], TextEdit::singleline(&mut self.connection_speed));
                            ui.label("bps");
                        });

                        ui.add_space(8.0);
                        ui.label("Max. connections number");
                        ui.horizontal(|ui| {
                            ui.label("Default max. conn. number:");
                            ComboBox::from_id_salt("max_conns_combo")
                                .selected_text(format!("{}", self.max_connections))
                                .show_ui(ui, |ui| {
                                    for count in [1, 2, 4, 8, 16, 24, 32] {
                                        ui.selectable_value(&mut self.max_connections, count, format!("{}", count));
                                    }
                                });
                        });

                        ui.add_space(8.0);
                        ui.group(|ui| {
                            ui.label("Exceptions:");
                            egui::Grid::new("exceptions_grid").min_col_width(120.0).show(ui, |ui| {
                                ui.strong("Server");
                                ui.strong("Num...");
                                ui.end_row();
                            });
                            ui.horizontal(|ui| {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let _ = ui.button("Edit");
                                    let _ = ui.button("Delete");
                                    let _ = ui.button("New");
                                });
                            });
                        });

                        ui.add_space(8.0);
                        ui.group(|ui| {
                            let mut dl_limit = true;
                            ui.add(Checkbox::new(&mut dl_limit, "Download limits"));
                            ui.horizontal(|ui| {
                                ui.label("Download no more then");
                                let mut mb_str = self.download_limit_mb.to_string();
                                if ui.add_sized([60.0, 20.0], TextEdit::singleline(&mut mb_str)).changed() {
                                    if let Ok(v) = mb_str.parse::<u64>() { self.download_limit_mb = v; }
                                }
                                ui.label("MBytes");
                            });
                            ui.horizontal(|ui| {
                                ui.label("every");
                                let mut hr_str = self.download_limit_hours.to_string();
                                if ui.add_sized([40.0, 20.0], TextEdit::singleline(&mut hr_str)).changed() {
                                    if let Ok(v) = hr_str.parse::<u32>() { self.download_limit_hours = v; }
                                }
                                ui.label("hours");
                            });
                            ui.add(Checkbox::new(&mut self.show_limit_warning, "Show warning before stopping downloads"));
                        });
                    }
                    OptionsTab::Proxy => {
                        // Verified from proxy.png
                        ui.heading("HTTP, HTTPS and FTP proxy configuration");
                        ui.add_space(4.0);

                        for (label, check_label) in [
                            ("HTTP", "Use HTTP Proxy"),
                            ("HTTPS", "Use HTTPS Proxy"),
                            ("FTP", "Use FTP Proxy"),
                        ] {
                            let mut use_proxy = false;
                            ui.add(Checkbox::new(&mut use_proxy, check_label));
                            ui.horizontal(|ui| {
                                let mut addr = String::new();
                                let mut port = String::from("8000");
                                let mut user = String::new();
                                let mut pass = String::new();
                                ui.add_sized([130.0, 20.0], TextEdit::singleline(&mut addr).hint_text(format!("{} proxy address", label)));
                                ui.label("Port");
                                ui.add_sized([50.0, 20.0], TextEdit::singleline(&mut port));
                                ui.label("UserName");
                                ui.add_sized([70.0, 20.0], TextEdit::singleline(&mut user));
                                ui.label("Password");
                                ui.add_sized([70.0, 20.0], TextEdit::singleline(&mut pass).password(true));
                            });
                            ui.horizontal(|ui| {
                                let _ = ui.button("Get from IE");
                                let _ = ui.button("Confirm password");
                            });
                            ui.add_space(4.0);
                        }

                        let mut use_pasv = false;
                        ui.add(Checkbox::new(&mut use_pasv, "Use FTP in PASV mode"));
                        ui.label("Do not use proxy server for addresses beginning with:");
                        let mut no_proxy = String::from("localhost");
                        ui.add_sized([ui.available_width() - 8.0, 20.0], TextEdit::singleline(&mut no_proxy));
                        ui.small("Separate names by spaces");
                    }
                    OptionsTab::SitesLogins => {
                        // Verified from logins.png
                        ui.heading("Sites authentication");
                        ui.add_space(4.0);
                        ui.label("Default login and password for sites requiring authorization:");
                        ui.add_space(4.0);
                        egui::Grid::new("logins_grid").min_col_width(100.0).striped(true).show(ui, |ui| {
                            ui.strong("URL/Server");
                            ui.strong("Username");
                            ui.strong("Password");
                            ui.end_row();
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let _ = ui.button("New");
                            let _ = ui.button("Edit");
                            let _ = ui.button("Delete");
                        });
                    }
                    OptionsTab::DialUp => {
                        // Verified from dialup.png
                        ui.heading("Dial-Up/VPN Connection settings");
                        ui.add_space(4.0);
                        ui.label("These settings are for legacy dial-up connections.");
                        ui.label("Modern broadband connections typically do not need these settings.");
                        ui.add_space(8.0);
                        let mut auto_dial = false;
                        ui.add(Checkbox::new(&mut auto_dial, "Automatically dial the connection when needed"));
                        let mut auto_hangup = false;
                        ui.add(Checkbox::new(&mut auto_hangup, "Hang up when done"));
                    }
                    OptionsTab::Sounds => {
                        // Verified from sounds.png
                        ui.heading("Sound settings");
                        ui.add_space(4.0);
                        ui.label("Select sounds for Internet Download Manager events");
                        ui.add_space(4.0);
                        egui::Grid::new("sounds_grid").min_col_width(200.0).striped(true).show(ui, |ui| {
                            ui.strong("Event");
                            ui.strong("Sound file");
                            ui.end_row();
                            ui.add(Checkbox::new(&mut self.sound_complete, "Download complete"));
                            ui.label("chimes.wav");
                            ui.end_row();
                            ui.add(Checkbox::new(&mut self.sound_failed, "Download failed"));
                            ui.label("ringln.wav");
                            ui.end_row();
                            ui.add(Checkbox::new(&mut self.sound_queue_started, "Queue processing started"));
                            ui.label("");
                            ui.end_row();
                            ui.add(Checkbox::new(&mut self.sound_queue_finished, "Queue processing stopped/finished"));
                            ui.label("");
                            ui.end_row();
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let _ = ui.button("Browse...");
                            ui.add_space(20.0);
                            let _ = ui.button("Play");
                        });
                    }
                }

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(6.0);

                // Bottom buttons: OK | Help (right-aligned), matching original IDM
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([60.0, 24.0], Button::new("Help")).clicked() {
                            // Help action placeholder
                        }

                        if ui.add_sized([60.0, 24.0], Button::new("OK")).clicked() {
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
