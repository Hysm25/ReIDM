use crate::engine::ProgressUpdate;
use colored::*;
use std::io::{stdout, Write};

pub struct TerminalProgress;

impl TerminalProgress {
    /// Renders IDM-style terminal status line and segmented visual bar
    pub fn render(update: &ProgressUpdate, width: usize) {
        let percent = if update.total_bytes == 0 {
            0.0
        } else {
            (update.downloaded_bytes as f64 / update.total_bytes as f64 * 100.0).clamp(0.0, 100.0)
        };

        let speed_str = format_speed(update.speed_bps);
        let downloaded_str = format_bytes(update.downloaded_bytes);
        let total_str = format_bytes(update.total_bytes);

        let eta_str = if update.speed_bps > 0 && update.total_bytes > update.downloaded_bytes {
            let rem_sec = (update.total_bytes - update.downloaded_bytes) / update.speed_bps;
            format_duration(rem_sec)
        } else {
            "--:--".to_string()
        };

        // Render IDM-style segmented progress bar
        let bar_width = width.max(40);
        let segmented_bar = render_segmented_bar(update, bar_width);

        // Header info line
        print!(
            "\r[{:6.2}%] {} / {} | {} | ETA: {} | {} conns\n{}\x1b[1A",
            percent,
            downloaded_str.cyan(),
            total_str.green(),
            speed_str.yellow().bold(),
            eta_str.white(),
            update.active_connections.to_string().magenta(),
            segmented_bar
        );
        let _ = stdout().flush();
    }

    pub fn finish() {
        println!();
        println!("{}", "Download finished successfully!".green().bold());
    }
}

/// Renders the famous IDM segmented bar:
/// Pink '|' = segment boundary
/// Blue '█' = downloaded segment portion
/// White '░' = pending segment portion
fn render_segmented_bar(update: &ProgressUpdate, width: usize) -> String {
    if update.total_bytes == 0 || update.segments.is_empty() {
        let filled = (width as f64 * (update.downloaded_bytes as f64 / 1.0).min(1.0)) as usize;
        let bar: String = "█".repeat(filled) + &"░".repeat(width.saturating_sub(filled));
        return format!("[{}]", bar.blue());
    }

    let mut cells = vec!['░'; width];
    let total = update.total_bytes as f64;

    for seg in &update.segments {
        let start_idx = ((seg.start_pos as f64 / total) * (width as f64)) as usize;
        let curr_idx = ((seg.current_pos as f64 / total) * (width as f64)) as usize;
        let end_idx = ((seg.end_pos as f64 / total) * (width as f64)).min(width as f64 - 1.0) as usize;

        // Fill downloaded portion in blue
        for i in start_idx..=curr_idx.min(end_idx) {
            if i < width {
                cells[i] = '█';
            }
        }

        // Pink start mark
        if start_idx < width {
            cells[start_idx] = '|';
        }
    }

    let bar_str: String = cells
        .into_iter()
        .map(|c| match c {
            '|' => format!("{}", "|".bright_magenta().bold()),
            '█' => format!("{}", "█".bright_blue()),
            _ => format!("{}", "░".bright_black()),
        })
        .collect();

    format!("[{}]", bar_str)
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_speed(bps: u64) -> String {
    format!("{}/s", format_bytes(bps))
}

fn format_duration(seconds: u64) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    let hours = mins / 60;
    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, mins % 60, secs)
    } else {
        format!("{:02}:{:02}", mins, secs)
    }
}
