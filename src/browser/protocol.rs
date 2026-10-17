use std::collections::HashMap;

/// Parsed message from the IDM browser extension
#[derive(Debug, Clone)]
pub struct IdmMessage {
    pub seq: u64,
    pub msg_type: u32,
    #[allow(dead_code)]
    pub action: u32,
    #[allow(dead_code)]
    pub flags: u32,
    pub positional_args: Vec<String>,
    pub key_values: HashMap<u32, String>,
}

impl IdmMessage {
    /// Parses an IDM extension wire frame: MSG#seq#type#action#flags[:p1:p2...][,k=v...];
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim().trim_end_matches(';');
        if !trimmed.starts_with("MSG#") {
            return None;
        }

        // Split into the header part (before first comma or colon after flags) and the rest
        let parts: Vec<&str> = trimmed.splitn(5, '#').collect();
        if parts.len() < 5 {
            return None;
        }

        let seq = parts[1].parse::<u64>().ok()?;
        let msg_type = parts[2].parse::<u32>().ok()?;
        let action = parts[3].parse::<u32>().ok()?;

        let rest = parts[4];
        let mut positional_args = Vec::new();
        let mut key_values = HashMap::new();

        // The flags is up to the first ':' or ',' or end of string
        let (flags_str, remainder) = if let Some(idx) = rest.find(|c| c == ':' || c == ',') {
            (&rest[..idx], &rest[idx..])
        } else {
            (rest, "")
        };
        let flags = flags_str.parse::<u32>().unwrap_or(0);

        // Parse remainder: can have :arg1:arg2 followed by ,k=v...
        let mut chars = remainder;
        if chars.starts_with(':') {
            chars = &chars[1..];
            // Split up to ','
            let (pos_part, kv_part) = if let Some(comma_idx) = chars.find(',') {
                (&chars[..comma_idx], &chars[comma_idx..])
            } else {
                (chars, "")
            };

            for p in pos_part.split(':') {
                positional_args.push(p.to_string());
            }
            chars = kv_part;
        }

        // Parse key-value pairs: ,key=val or ,key=len:val
        while !chars.is_empty() {
            if chars.starts_with(',') {
                chars = &chars[1..];
            }
            if chars.is_empty() {
                break;
            }

            // Find next '='
            if let Some(eq_idx) = chars.find('=') {
                let key_str = &chars[..eq_idx];
                if let Ok(key) = key_str.parse::<u32>() {
                    chars = &chars[eq_idx + 1..];

                    // Check if value is length-prefixed: <len>:<value>
                    // where <len> is digits before a colon
                    if let Some(colon_idx) = chars.find(':') {
                        let potential_len = &chars[..colon_idx];
                        if potential_len.chars().all(|c| c.is_ascii_digit()) && !potential_len.is_empty() {
                            if let Ok(len) = potential_len.parse::<usize>() {
                                let val_start = colon_idx + 1;
                                if chars.len() >= val_start + len {
                                    let val = &chars[val_start..val_start + len];
                                    key_values.insert(key, val.to_string());
                                    chars = &chars[val_start + len..];
                                    continue;
                                }
                            }
                        }
                    }

                    // Otherwise, value goes until next comma or end
                    let next_comma = chars.find(',').unwrap_or(chars.len());
                    let val = &chars[..next_comma];
                    key_values.insert(key, val.to_string());
                    chars = &chars[next_comma..];
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Some(Self {
            seq,
            msg_type,
            action,
            flags,
            positional_args,
            key_values,
        })
    }

    /// Creates an IDM protocol handshake response (Type 3)
    pub fn handshake_response(seq: u64) -> String {
        format!("MSG#{}#3#0#0:113:0:0:0:96:0:0:0:100;", seq)
    }

    /// Creates an IDM protocol configuration update (Type 4)
    pub fn config_update(seq: u64) -> String {
        let ext_regex = "(?:ZIP|RAR|EXE|MSI|TAR|GZ|TGZ|7Z|MP3|MP4|AVI|MKV|ISO|PDF|DOC|DOCX|XLS|XLSX|PPT|PPTX|BIN|DMG|DEB|RPM|APK)";
        format!(
            "MSG#{}#4#0#0:0:0:0:0,1={}:{};",
            seq,
            ext_regex.len(),
            ext_regex
        )
    }

    /// Creates a keepalive ping ack (Type 1)
    pub fn ping_ack(seq: u64) -> String {
        format!("MSG#{}#1#1#0:1;", seq)
    }

    /// Creates a download takeover response (Type 13)
    pub fn download_takeover(seq: u64, download_id: &str) -> String {
        format!("MSG#{}#13#1#0:{}:3;", seq, download_id)
    }
}
