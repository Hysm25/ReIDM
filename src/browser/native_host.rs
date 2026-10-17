use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::{stdin, stdout, Read, Write};

#[derive(Debug, Serialize, Deserialize)]
pub struct NativeMessageIn {
    pub url: Option<String>,
    #[serde(rename = "userAgent")]
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub cookies: Option<String>,
    #[serde(rename = "postData")]
    pub post_data: Option<String>,
    pub filename: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NativeMessageOut {
    pub cancel: bool,
    pub takeover: bool,
    pub message: String,
}

pub struct NativeMessagingHost;

impl NativeMessagingHost {
    /// Reads a length-prefixed message from stdin (JSON object or string)
    pub fn read_message() -> std::io::Result<Option<NativeMessageIn>> {
        let mut handle = stdin().lock();
        let len = match handle.read_u32::<LittleEndian>() {
            Ok(l) => l as usize,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e),
        };

        if len == 0 || len > 10 * 1024 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid native message length",
            ));
        }

        let mut buf = vec![0u8; len];
        handle.read_exact(&mut buf)?;

        // 1. Try standard JSON object format
        if let Ok(msg) = serde_json::from_slice::<NativeMessageIn>(&buf) {
            return Ok(Some(msg));
        }

        // 2. Try JSON string containing IDM MSG# frame: "\"MSG#...\""
        if let Ok(raw_str) = serde_json::from_slice::<String>(&buf) {
            if let Some(idm_msg) = super::protocol::IdmMessage::parse(&raw_str) {
                // Respond to handshake or ping if needed
                match idm_msg.msg_type {
                    2 => {
                        let resp1 = super::protocol::IdmMessage::handshake_response(idm_msg.seq);
                        let _ = Self::send_string_response(&resp1);
                        let resp2 = super::protocol::IdmMessage::config_update(idm_msg.seq + 1);
                        let _ = Self::send_string_response(&resp2);
                    }
                    0 => {
                        let ack = super::protocol::IdmMessage::ping_ack(idm_msg.seq);
                        let _ = Self::send_string_response(&ack);
                    }
                    13 | 14 => {
                        let url = idm_msg.key_values.get(&6).cloned().or_else(|| idm_msg.key_values.get(&17).cloned());
                        let referer = idm_msg.key_values.get(&50).cloned();
                        let cookies = idm_msg.key_values.get(&51).cloned();
                        let user_agent = idm_msg.key_values.get(&54).cloned();
                        let filename = idm_msg.key_values.get(&100).cloned().or_else(|| idm_msg.key_values.get(&4).cloned());

                        if idm_msg.msg_type == 13 {
                            let dl_id = idm_msg.positional_args.first().map(|s| s.as_str()).unwrap_or("0");
                            let takeover = super::protocol::IdmMessage::download_takeover(idm_msg.seq, dl_id);
                            let _ = Self::send_string_response(&takeover);
                        } else {
                            let ack = super::protocol::IdmMessage::ping_ack(idm_msg.seq);
                            let _ = Self::send_string_response(&ack);
                        }

                        return Ok(Some(NativeMessageIn {
                            url,
                            user_agent,
                            referer,
                            cookies,
                            post_data: None,
                            filename,
                        }));
                    }
                    _ => {
                        let ack = super::protocol::IdmMessage::ping_ack(idm_msg.seq);
                        let _ = Self::send_string_response(&ack);
                    }
                }
            }
        }

        Ok(None)
    }

    /// Sends a length-prefixed JSON response to stdout
    pub fn send_response(resp: &NativeMessageOut) -> std::io::Result<()> {
        let json_bytes = serde_json::to_vec(resp)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let mut handle = stdout().lock();
        handle.write_u32::<LittleEndian>(json_bytes.len() as u32)?;
        handle.write_all(&json_bytes)?;
        handle.flush()?;
        Ok(())
    }

    /// Sends a length-prefixed JSON string response to stdout
    pub fn send_string_response(msg: &str) -> std::io::Result<()> {
        let json_bytes = serde_json::to_vec(msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let mut handle = stdout().lock();
        handle.write_u32::<LittleEndian>(json_bytes.len() as u32)?;
        handle.write_all(&json_bytes)?;
        handle.flush()?;
        Ok(())
    }
}
