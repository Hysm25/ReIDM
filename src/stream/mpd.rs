use futures_util::StreamExt;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use reqwest::Client;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use url::Url;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DashSegment {
    pub url: String,
    pub is_init: bool,
}

#[allow(dead_code)]
pub struct MpdParser;

#[allow(dead_code)]
impl MpdParser {
    /// Parses an MPD XML manifest and generates segment URLs based on SegmentTemplate
    pub fn parse(xml_content: &str, base_url: &str) -> Result<Vec<DashSegment>, Box<dyn std::error::Error + Send + Sync>> {
        let base = Url::parse(base_url)?;
        let mut reader = Reader::from_str(xml_content);
        reader.config_mut().trim_text(true);

        let mut segments = Vec::new();
        let mut media_template = String::new();
        let mut init_template = String::new();
        let mut start_number: u64 = 1;

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    let name = e.name();
                    if name.as_ref() == b"SegmentTemplate" {
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"media" => {
                                    media_template = String::from_utf8_lossy(&attr.value).to_string();
                                }
                                b"initialization" => {
                                    init_template = String::from_utf8_lossy(&attr.value).to_string();
                                }
                                b"startNumber" => {
                                    start_number = String::from_utf8_lossy(&attr.value)
                                        .parse::<u64>()
                                        .unwrap_or(1);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(Box::new(e)),
                _ => {}
            }
            buf.clear();
        }

        // Add initialization segment if found
        if !init_template.is_empty() {
            let init_url = base.join(&init_template)?.to_string();
            segments.push(DashSegment {
                url: init_url,
                is_init: true,
            });
        }

        // Generate sample segment numbers if template found
        if !media_template.is_empty() {
            for num in start_number..start_number + 10 {
                let resolved = media_template
                    .replace("$Number$", &num.to_string())
                    .replace("$Number%05d$", &format!("{:05}", num))
                    .replace("$RepresentationID$", "1");
                let seg_url = base.join(&resolved)?.to_string();
                segments.push(DashSegment {
                    url: seg_url,
                    is_init: false,
                });
            }
        }

        Ok(segments)
    }

    /// Downloads DASH segments and stitches into target file
    pub async fn download_dash(
        client: &Client,
        segments: &[DashSegment],
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

        for seg in segments {
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
