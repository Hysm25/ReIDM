use crate::engine::segmenter::SegmentProgress;
use crate::ui::theme::IdmTheme;
use eframe::egui::{Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, Widget};

pub struct SegmentVisualizer<'a> {
    total_bytes: u64,
    segments: &'a [SegmentProgress],
    height: f32,
}

impl<'a> SegmentVisualizer<'a> {
    pub fn new(total_bytes: u64, segments: &'a [SegmentProgress]) -> Self {
        Self {
            total_bytes,
            segments,
            height: 24.0,
        }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }
}

impl<'a> Widget for SegmentVisualizer<'a> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), self.height),
            Sense::hover(),
        );

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();

            // 1. Sunken dark background
            painter.rect_filled(rect, 0.0, IdmTheme::SEGMENT_BG);
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, IdmTheme::WIN_BORDER));

            if self.total_bytes > 0 {
                let total = self.total_bytes as f32;
                let w = rect.width();

                // 2. Draw each segment's downloaded blocks
                for seg in self.segments {
                    let start_ratio = (seg.start_pos as f32 / total).clamp(0.0, 1.0);
                    let curr_ratio = (seg.current_pos as f32 / total).clamp(0.0, 1.0);

                    let x_start = rect.min.x + start_ratio * w;
                    let x_curr = rect.min.x + curr_ratio * w;

                    // Fill downloaded block
                    if x_curr > x_start {
                        let fill_rect = Rect::from_min_max(
                            Pos2::new(x_start, rect.min.y + 1.0),
                            Pos2::new(x_curr, rect.max.y - 1.0),
                        );
                        painter.rect_filled(fill_rect, 0.0, IdmTheme::SEGMENT_FILLED);
                    }

                    // Pink vertical divider mark at the segment start position
                    let mark_top = Pos2::new(x_start, rect.min.y);
                    let mark_bottom = Pos2::new(x_start, rect.max.y);
                    painter.line_segment(
                        [mark_top, mark_bottom],
                        Stroke::new(2.0_f32, IdmTheme::SEGMENT_PINK_MARKER),
                    );
                }
            }
        }

        response
    }
}
