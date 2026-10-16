use crate::types::Segment;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub struct Assembler;

impl Assembler {
    /// Assembles segments into the final destination file following the linked-list order
    pub fn assemble_segments(
        segments: &[Segment],
        destination: &Path,
        expected_size: u64,
    ) -> std::io::Result<u64> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }

        // Use a .result.tmp temporary file to guarantee atomic completion
        let tmp_dest = destination.with_extension(format!(
            "{}.result.tmp",
            destination
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("dat")
        ));

        let mut out_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_dest)?;

        // 1. Sort segments by starting position to guarantee correct linear assembly
        let mut ordered_segments: Vec<&Segment> = segments.iter().collect();
        ordered_segments.sort_by_key(|s| s.start_pos);

        let mut total_written: u64 = 0;
        let mut buffer = vec![0u8; 128 * 1024]; // 128 KB copy buffer

        for seg in ordered_segments {
            if !seg.temp_file.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Missing segment chunk file: {:?}", seg.temp_file),
                ));
            }

            let mut chunk_file = File::open(&seg.temp_file)?;
            let chunk_len = chunk_file.metadata()?.len();

            let mut chunk_read: u64 = 0;
            while chunk_read < chunk_len {
                let to_read = buffer.len().min((chunk_len - chunk_read) as usize);
                let n = chunk_file.read(&mut buffer[..to_read])?;
                if n == 0 {
                    break;
                }
                out_file.write_all(&buffer[..n])?;
                chunk_read += n as u64;
                total_written += n as u64;
            }
        }

        out_file.flush()?;
        drop(out_file);

        // Verification of assembled size
        if expected_size > 0 && total_written != expected_size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Assembled file size mismatch: got {} bytes, expected {} bytes",
                    total_written, expected_size
                ),
            ));
        }

        // Atomically rename to final destination
        fs::rename(&tmp_dest, destination)?;

        Ok(total_written)
    }
}
