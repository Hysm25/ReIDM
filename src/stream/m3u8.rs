use futures_util::StreamExt;
use reqwest::Client;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use url::Url;

#[derive(Debug, Clone)]
pub struct HlsSegment {
    pub url: String,
    pub duration: f64,
}

pub struct M3u8Parser;

impl M3u8Parser {
    /// Parses an M3U8 playlist content and resolves segment URLs relative to base_url
    pub fn parse(content: &str, base_url: &str) -> Result<Vec<HlsSegment>, Box<dyn std::error::Error + Send + Sync>> {
        let base = Url::parse(base_url)?;
        let mut segments = Vec::new();
        let mut current_duration = 0.0;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with("#EXTINF:") {
                let dur_part = trimmed.trim_start_matches("#EXTINF:").split(',').next().unwrap_or("0");
                current_duration = dur_part.trim().parse::<f64>().unwrap_or(0.0);
            } else if !trimmed.starts_with('#') {
                // Segment URL
                let seg_url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                    trimmed.to_string()
                } else {
                    base.join(trimmed)?.to_string()
                };

                segments.push(HlsSegment {
                    url: seg_url,
                    duration: current_duration,
                });
                current_duration = 0.0;
            }
        }

        Ok(segments)
    }

    /// Downloads and concatenates all HLS segments into the target video file
    pub async fn download_hls(
        client: &Client,
        segments: &[HlsSegment],
        destination: &Path,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut out_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(destination)?;

        let mut total_bytes: u64 = 0;

        for (i, seg) in segments.iter().enumerate() {
            println!("Downloading segment {}/{} ({}s)", i + 1, segments.len(), seg.duration);
            let resp = client.get(&seg.url).send().await?;
            if !resp.status().is_success() {
                continue;
            }

            let mut stream = resp.bytes_stream();
            while let Some(chunk_res) = stream.next().await {
                let bytes = chunk_res?;
                out_file.write_all(&bytes)?;
                total_bytes += bytes.len() as u64;
            }
        }

        out_file.flush()?;
        Ok(total_bytes)
    }
}
