use eframe::egui::{self, Color32, Stroke, Vec2};

pub struct IdmTheme;

#[allow(dead_code)]
impl IdmTheme {
    // Classic IDM palette
    pub const WIN_GRAY: Color32 = Color32::from_rgb(236, 233, 216);      // #ECE9D8
    pub const WIN_LIGHT: Color32 = Color32::from_rgb(245, 245, 245);     // #F5F5F5
    pub const WIN_DARK: Color32 = Color32::from_rgb(128, 128, 128);      // #808080
    pub const WIN_BORDER: Color32 = Color32::from_rgb(160, 160, 160);    // #A0A0A0
    pub const WHITE: Color32 = Color32::from_rgb(255, 255, 255);
    pub const BLACK: Color32 = Color32::from_rgb(0, 0, 0);

    // Toolbar colors
    pub const TOOLBAR_BG: Color32 = Color32::from_rgb(255, 255, 255);
    pub const TOOLBAR_BG_CLASSIC: Color32 = Color32::from_rgb(238, 235, 222);
    pub const TOOLBAR_HOVER: Color32 = Color32::from_rgb(218, 234, 253);
    pub const TOOLBAR_ACTIVE: Color32 = Color32::from_rgb(176, 208, 248);
    pub const TOOLBAR_BORDER: Color32 = Color32::from_rgb(49, 106, 197);
    pub const STATUS_BAR_BG: Color32 = Color32::from_rgb(240, 240, 240);
    pub const STATUS_BAR_PANEL: Color32 = Color32::from_rgb(245, 245, 245);
    pub const STATUS_BAR_BORDER: Color32 = Color32::from_rgb(160, 160, 160);

    // Tree / Sidebar colors
    pub const TREE_BG: Color32 = Color32::from_rgb(255, 255, 255);
    pub const TREE_HEADER_BG: Color32 = Color32::from_rgb(238, 235, 224);
    pub const TREE_LINE: Color32 = Color32::from_rgb(180, 180, 180);

    // Table colors
    pub const TABLE_HEADER_BG: Color32 = Color32::from_rgb(242, 242, 242);
    pub const TABLE_GRID: Color32 = Color32::from_rgb(230, 230, 230);
    pub const SELECTION_BLUE: Color32 = Color32::from_rgb(49, 106, 197); // Windows selection blue
    pub const STATUS_COMPLETE: Color32 = Color32::from_rgb(0, 128, 0);   // Green
    pub const STATUS_DOWNLOADING: Color32 = Color32::from_rgb(0, 102, 204); // Blue
    pub const STATUS_PAUSED: Color32 = Color32::from_rgb(204, 102, 0);   // Orange
    pub const STATUS_ERROR: Color32 = Color32::from_rgb(204, 0, 0);      // Red

    // Segmented progress visualizer colors (Verified from original IDM segment screenshots)
    pub const SEGMENT_BG: Color32 = Color32::from_rgb(255, 255, 255);       // Pure white container (#FFFFFF)
    pub const SEGMENT_FILLED: Color32 = Color32::from_rgb(0, 0, 255);        // Pure saturated blue (#0000FF)
    pub const SEGMENT_PINK_MARKER: Color32 = Color32::from_rgb(255, 0, 255); // Pure magenta start line (#FF00FF)
    pub const PROGRESS_BAR_FILL: Color32 = Color32::from_rgb(16, 180, 60);  // Classic Windows green

    /// Applies IDM visual style to egui context
    pub fn apply(ctx: &egui::Context) {
        let mut visuals = egui::Visuals::light();
        visuals.override_text_color = Some(Self::BLACK);
        visuals.window_fill = Self::WIN_GRAY;
        visuals.panel_fill = Self::WHITE;
        visuals.faint_bg_color = Self::TABLE_HEADER_BG;
        visuals.extreme_bg_color = Self::WHITE;
        visuals.widgets.noninteractive.bg_fill = Self::WHITE;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Self::WIN_BORDER);
        visuals.widgets.inactive.bg_fill = Self::WIN_LIGHT;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Self::WIN_BORDER);
        visuals.widgets.hovered.bg_fill = Self::TOOLBAR_HOVER;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Self::TOOLBAR_BORDER);
        visuals.widgets.active.bg_fill = Self::TOOLBAR_ACTIVE;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, Self::TOOLBAR_BORDER);

        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(4.0, 4.0);
        style.spacing.button_padding = Vec2::new(6.0, 3.0);
        ctx.set_style(style);
    }
}
