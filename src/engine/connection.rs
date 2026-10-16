use reqwest::header::{
    ACCEPT, ACCEPT_CHARSET, ACCEPT_ENCODING, ACCEPT_LANGUAGE,
    ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, COOKIE, ETAG,
    LAST_MODIFIED, RANGE, REFERER, USER_AGENT,
};
use reqwest::{Client, StatusCode};
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub file_size: u64,
    pub resumable: bool,
    pub file_name: String,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

pub struct ConnectionManager {
    client: Client,
    user_agent: String,
}

impl ConnectionManager {
    pub fn new(user_agent: String, timeout_secs: u64) -> reqwest::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .pool_max_idle_per_host(32)
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_nodelay(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()?;

        Ok(Self { client, user_agent })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Probes the server for range support, file size, headers, and filename
    pub async fn probe(
        &self,
        url_str: &str,
        referer: Option<&str>,
        cookies: Option<&str>,
    ) -> Result<ProbeResult, Box<dyn std::error::Error + Send + Sync>> {
        let parsed_url = Url::parse(url_str)?;
        let mut default_filename = parsed_url
            .path_segments()
            .and_then(|mut segs| segs.next_back())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "download".to_string());

        // Try HEAD request first with standard IDM headers
        let mut head_req = self.client.head(url_str);
        head_req = self.apply_standard_headers(head_req, referer, cookies);

        let head_res = head_req.send().await;

        let response = match head_res {
            Ok(resp) if resp.status().is_success() => resp,
            _ => {
                // If HEAD fails or is rejected, perform GET with Range: bytes=0-0 to probe range support
                let mut get_req = self.client.get(url_str);
                get_req = self.apply_standard_headers(get_req, referer, cookies);
                get_req = get_req.header(RANGE, "bytes=0-0");
                get_req.send().await?
            }
        };

        let headers = response.headers().clone();

        // 1. Check Range support
        let mut resumable = false;
        let mut file_size: u64 = 0;

        if let Some(accept_ranges) = headers.get(ACCEPT_RANGES) {
            if let Ok(val) = accept_ranges.to_str() {
                if val.to_lowercase().contains("bytes") {
                    resumable = true;
                }
            }
        }

        if response.status() == StatusCode::PARTIAL_CONTENT {
            resumable = true;
            if let Some(content_range) = headers.get(CONTENT_RANGE) {
                if let Ok(val) = content_range.to_str() {
                    // bytes 0-0/total
                    if let Some(slash_idx) = val.find('/') {
                        let total_part = &val[slash_idx + 1..];
                        if let Ok(parsed) = total_part.trim().parse::<u64>() {
                            file_size = parsed;
                        }
                    }
                }
            }
        }

        if file_size == 0 {
            if let Some(cl) = headers.get(CONTENT_LENGTH) {
                if let Ok(val) = cl.to_str() {
                    file_size = val.trim().parse::<u64>().unwrap_or(0);
                }
            }
        }

        // 2. Parse filename from Content-Disposition if present
        if let Some(cd) = headers.get(CONTENT_DISPOSITION) {
            if let Ok(val) = cd.to_str() {
                if let Some(extracted) = parse_content_disposition_filename(val) {
                    default_filename = extracted;
                }
            }
        }

        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let etag = headers
            .get(ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let last_modified = headers
            .get(LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Ok(ProbeResult {
            file_size,
            resumable,
            file_name: sanitize_filename(&default_filename),
            content_type,
            etag,
            last_modified,
        })
    }

    /// Prepares a segment GET request with Range: bytes={start}-{end}
    pub fn build_range_request(
        &self,
        url: &str,
        start_pos: u64,
        end_pos: u64,
        referer: Option<&str>,
        cookies: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let mut req = self.client.get(url);
        req = self.apply_standard_headers(req, referer, cookies);
        req = req.header(RANGE, format!("bytes={}-{}", start_pos, end_pos));
        req
    }

    /// Prepares a full GET request for non-resumable single-stream downloads
    pub fn build_full_request(
        &self,
        url: &str,
        referer: Option<&str>,
        cookies: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let req = self.client.get(url);
        self.apply_standard_headers(req, referer, cookies)
    }

    fn apply_standard_headers(
        &self,
        mut req: reqwest::RequestBuilder,
        referer: Option<&str>,
        cookies: Option<&str>,
    ) -> reqwest::RequestBuilder {
        req = req.header(USER_AGENT, &self.user_agent);
        req = req.header(ACCEPT, "*/*");
        req = req.header(ACCEPT_ENCODING, "identity");
        req = req.header(ACCEPT_LANGUAGE, "en-US,en;q=0.9");
        req = req.header(ACCEPT_CHARSET, "*");

        if let Some(ref_url) = referer {
            req = req.header(REFERER, ref_url);
        }
        if let Some(cookie_val) = cookies {
            req = req.header(COOKIE, cookie_val);
        }
        req
    }
}

/// Parses Content-Disposition according to RFC 6266 / RFC 5987
fn parse_content_disposition_filename(cd: &str) -> Option<String> {
    // 1. Look for filename*=UTF-8''...
    if let Some(idx) = cd.to_lowercase().find("filename*=") {
        let sub = &cd[idx + 10..];
        let val = sub.split(';').next()?.trim();
        if let Some(utf8_idx) = val.to_lowercase().find("utf-8''") {
            let encoded = &val[utf8_idx + 7..];
            if let Ok(decoded) = percent_encoding::percent_decode_str(encoded).decode_utf8() {
                return Some(decoded.trim_matches('"').to_string());
            }
        }
    }

    // 2. Look for filename="..."
    if let Some(idx) = cd.to_lowercase().find("filename=") {
        let sub = &cd[idx + 9..];
        let val = sub.split(';').next()?.trim().trim_matches('"');
        if !val.is_empty() {
            return Some(val.to_string());
        }
    }

    None
}

/// Sanitizes a string for safe filesystem usage
fn sanitize_filename(name: &str) -> String {
    let forbidden = ['/', '\\', '?', '%', '*', ':', '|', '"', '<', '>', '\0'];
    let filtered: String = name
        .chars()
        .map(|c| if forbidden.contains(&c) { '_' } else { c })
        .collect();
    let trimmed = filtered.trim().to_string();
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed
    }
}
