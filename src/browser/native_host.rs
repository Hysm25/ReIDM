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
    /// Reads a length-prefixed JSON message from stdin (Standard WebExtension Native Messaging)
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

        let msg: NativeMessageIn = serde_json::from_slice(&buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(Some(msg))
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
}
