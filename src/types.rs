use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::PathBuf;

/// Exact 16-byte chunk record structure matching Internet Download Manager's internal binary layout
/// [Software\DownloadManager\{id}\ChList]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkRecord {
    pub chunk_id: u32,
    pub next_id: i32,    // -1 = end of linked list
    pub start_pos: u64,  // 64-bit starting byte offset
}

impl ChunkRecord {
    pub const SIZE: usize = 16;

    pub fn new(chunk_id: u32, next_id: i32, start_pos: u64) -> Self {
        Self {
            chunk_id,
            next_id,
            start_pos,
        }
    }

    /// Serializes to the exact 16-byte little-endian format verified in IDM
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        let mut cur = Cursor::new(&mut buf[..]);
        cur.write_u32::<LittleEndian>(self.chunk_id).unwrap();
        cur.write_i32::<LittleEndian>(self.next_id).unwrap();
        cur.write_u64::<LittleEndian>(self.start_pos).unwrap();
        buf
    }

    /// Deserializes from a 16-byte little-endian buffer
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        if bytes.len() < 16 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Need 16 bytes for ChunkRecord",
            ));
        }
        let mut cur = Cursor::new(bytes);
        let chunk_id = cur.read_u32::<LittleEndian>()?;
        let next_id = cur.read_i32::<LittleEndian>()?;
        let start_pos = cur.read_u64::<LittleEndian>()?;
        Ok(Self {
            chunk_id,
            next_id,
            start_pos,
        })
    }
}

/// Download states corresponding to IDM status codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadStatus {
    Idle = 0,
    Connecting = 1,
    Downloading = 2,
    Complete = 3,
    Paused = 4,
    Error = 5,
}

impl std::fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadStatus::Idle => write!(f, "Idle"),
            DownloadStatus::Connecting => write!(f, "Connecting"),
            DownloadStatus::Downloading => write!(f, "Downloading"),
            DownloadStatus::Complete => write!(f, "Complete"),
            DownloadStatus::Paused => write!(f, "Paused"),
            DownloadStatus::Error => write!(f, "Error"),
        }
    }
}

/// Dynamic active segment representing a concurrent download chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: u32,
    pub start_pos: u64,
    pub current_pos: u64,
    pub end_pos: u64, // inclusive
    pub downloaded: u64,
    pub next_id: i32,
    pub temp_file: PathBuf,
    pub is_active: bool,
}

impl Segment {
    pub const MIN_SPLIT_SIZE: u64 = 256 * 1024; // 256 KB minimum remaining to split

    pub fn new(id: u32, start_pos: u64, end_pos: u64, next_id: i32, temp_file: PathBuf) -> Self {
        Self {
            id,
            start_pos,
            current_pos: start_pos,
            end_pos,
            downloaded: 0,
            next_id,
            temp_file,
            is_active: false,
        }
    }

    /// Number of bytes remaining to be downloaded in this segment
    pub fn remaining(&self) -> u64 {
        if self.current_pos > self.end_pos {
            0
        } else {
            self.end_pos - self.current_pos + 1
        }
    }

    pub fn is_finished(&self) -> bool {
        self.current_pos > self.end_pos
    }

    /// Splits this segment in half according to IDM's verified in-half division rule:
    /// Midpoint = current_pos + remaining / 2
    /// The current segment retains [start_pos .. mid - 1]
    /// The new segment takes [mid .. end_pos] with next_id = current.next_id
    pub fn split_half(&mut self, new_id: u32, new_temp_file: PathBuf) -> Option<Segment> {
        let rem = self.remaining();
        if rem < Self::MIN_SPLIT_SIZE {
            return None;
        }

        let half = rem / 2;
        let mid = self.current_pos + half;

        let new_segment = Segment {
            id: new_id,
            start_pos: mid,
            current_pos: mid,
            end_pos: self.end_pos,
            downloaded: 0,
            next_id: self.next_id,
            temp_file: new_temp_file,
            is_active: false,
        };

        self.end_pos = mid - 1;
        self.next_id = new_id as i32;

        Some(new_segment)
    }

    pub fn to_chunk_record(&self) -> ChunkRecord {
        ChunkRecord {
            chunk_id: self.id,
            next_id: self.next_id,
            start_pos: self.start_pos,
        }
    }
}

/// Metadata and state for a single download item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: u64,
    pub url: String,
    pub file_name: String,
    pub local_path: PathBuf,
    pub category: String,
    pub file_size: u64,
    pub downloaded: u64,
    pub status: DownloadStatus,
    pub resumable: bool,
    pub user_agent: String,
    pub referer: Option<String>,
    pub cookies: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub temp_dir: PathBuf,
    pub segments: Vec<Segment>,
}

impl DownloadItem {
    pub fn final_destination(&self) -> PathBuf {
        self.local_path.join(&self.file_name)
    }

    pub fn percent_complete(&self) -> f64 {
        if self.file_size == 0 {
            0.0
        } else {
            (self.downloaded as f64 / self.file_size as f64 * 100.0).clamp(0.0, 100.0)
        }
    }
}
