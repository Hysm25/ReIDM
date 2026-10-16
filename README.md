# ReIDM: Internet Download Manager Re-Engineered in Rust

ReIDM is a high-performance download accelerator reverse-engineered from Tonec's **Internet Download Manager (IDM)** and ported to modern **Rust**. It faithfully replicates IDM's internal download acceleration architecture, dynamic in-half segmentation algorithm, connection reuse patterns, binary state representation, and browser extension communication protocols while delivering memory safety, cross-platform portability, and native asynchronous I/O with Tokio.

---

## Key Architecture & Verified IDM Mechanisms

### 1. Dynamic In-Half Segmentation
Unlike conventional download managers that divide a file into fixed equal slices at start (which causes "tail starvation" when slower connections stall), IDM uses **dynamic in-half division**:
- Downloads begin with segment 0 spanning `[0 .. file_size - 1]`.
- As concurrency slots become available (up to configured connection cap, e.g., 8–32), IDM scans active segments and finds the one with the **largest remaining un-downloaded byte count**.
- The segment is bisected at its midpoint:
  ```text
  mid = current_pos + (remaining / 2)
  ```
- Segment `A` retains `[start_pos .. mid - 1]`, while the newly spawned segment `B` takes `[mid .. end_pos]`.
- In-flight byte clamping ensures that even when a division happens while packets are on the wire, the active socket terminates gracefully at the new boundary without data corruption.

### 2. 16-Byte Chunk Linked-List (`ChList`) Binary Format
Extracted directly from IDM's registry key (`HKCU\Software\DownloadManager\{id}\ChList`), each segment record is serialized as a 16-byte binary structure:
```
+--------------------+-------------------+--------------------+
| chunk_id (4B, u32) | next_id (4B, i32) | start_pos (8B, u64)|
+--------------------+-------------------+--------------------+
Total: 16 bytes (Little-Endian)
```
- `-1` in `next_id` denotes the tail segment.
- Chunk files are stored in `{temp_dir}/{filename}_{id}/` following IDM's exact naming convention: chunk 0 is `{filename}`, subsequent chunks are `{filename}{chunk_id}`.

### 3. Connection Pooling & Handshake Elimination
- Rather than tearing down TCP/TLS connections between chunk completions, connections are recycled within an HTTP keep-alive pool.
- IDM eliminates connection handshake overhead for split chunks.

### 4. Category Routing & Filename Collision Resolution
Files are automatically routed into standard IDM category subdirectories based on MIME type and file extension:
- **Compressed:** `.zip`, `.rar`, `.7z`, `.tar`, `.gz`, `.bz2`, `.xz`, `.iso`, `.dmg`
- **Documents:** `.pdf`, `.doc`, `.docx`, `.xls`, `.xlsx`, `.ppt`, `.pptx`, `.txt`, `.rtf`, `.odt`
- **Music:** `.mp3`, `.wav`, `.wma`, `.flac`, `.aac`, `.m4a`, `.ogg`, `.opus`
- **Programs:** `.exe`, `.msi`, `.deb`, `.rpm`, `.apk`, `.appimage`, `.dmg`
- **Video:** `.mp4`, `.mkv`, `.avi`, `.flv`, `.mov`, `.wmv`, `.webm`, `.ts`, `.m3u8`
- **Filename Collision:** Follows `name (1).ext`, `name (2).ext` numbering scheme.

### 5. Browser Extension Integration
ReIDM provides 100% wire-compatible integration with official IDM browser extensions (Chrome, Edge, Firefox, Brave):
- **WebExtension Native Messaging (`com.tonec.idm`):** Stdio 32-bit length-prefixed JSON protocol communicating with `reidm --native-messaging`.
- **WebSocket Daemon (`ws://127.0.0.1:1001`):** Subprotocol `plugin.v3.internetdownloadmanager.com` handling download interception frames (`MSG#seq#type#action...`).

### 6. Streaming Protocol Support
- **HLS (.m3u8):** Full parsing of EXT-X-TARGETDURATION, EXTINF, and sequential cryptographic segment stitching.
- **DASH (.mpd):** SegmentTimeline and SegmentTemplate XML manifest parsing.

---

## IDM Command-Line Compatibility

ReIDM matches IDM's official command line switches:

| IDM Flag | ReIDM Argument | Description |
|---|---|---|
| `/d URL` | `-d, --url <URL>` | Target download URL |
| `/p path` | `-p, --path <DIR>` | Target download directory |
| `/f filename` | `-f, --filename <NAME>` | Target local file name |
| `/q` | `-q, --quit` | Exit silently upon download completion |
| `/n` | `-n, --silent` | Silent mode (disables interactive terminal bar) |
| `/a` | `-a, --add-only` | Add file to queue without starting immediately |
| `/s` | `-s, --start-queue` | Start processing queued items sequentially |
| N/A | `-r, --resume <PATH>` | Resume paused/interrupted download task |
| N/A | `-c, --connections <N>` | Concurrency cap per download (1..32, default: 8) |
| N/A | `-l, --limit <KB/s>` | Bandwidth token-bucket rate limiter |
| N/A | `--daemon` | Launch WebSocket listener on `ws://127.0.0.1:1001` |
| N/A | `--native-messaging` | Run as `com.tonec.idm` WebExtension host |
| N/A | `--list-tasks` | List paused / saved download sessions |
| N/A | `--install-browser-integration` | Auto-install manifests into Chrome/Firefox/Edge |

---

## Installation & Building

### Requirements
- Rust toolchain (`rustc 1.80+`, `cargo`)
- OpenSSL development packages (`libssl-dev` or `openssl-devel`)

### Build
```bash
cargo build --release
```
The optimized binary will be created at `target/release/reidm`.

### Run Tests
```bash
cargo test
```

---

## Usage Examples

### 1. Download a File with 8 Connections
```bash
reidm -d "https://example.com/largefile.zip"
```

### 2. Download with Rate Limiting (e.g., 500 KB/s)
```bash
reidm -d "https://example.com/archive.iso" -l 500
```

### 3. Add Multiple Items to Queue and Execute
```bash
reidm -a -d "https://example.com/part1.rar"
reidm -a -d "https://example.com/part2.rar"
reidm -s
```

### 4. Interrupt & Resume
Press `Ctrl+C` at any time to pause the download. ReIDM flushes in-flight bytes to disk and updates `metadata.json` and `chunks.bin`.
To resume:
```bash
reidm -r ~/.local/share/reidm/temp/archive.iso_1792189309689
# or simply run again with the same URL:
reidm -d "https://example.com/archive.iso"
```

### 5. Install Browser Extension Integration
```bash
reidm --install-browser-integration
```
This automatically registers the `com.tonec.idm.json` native messaging host with:
- Google Chrome
- Chromium
- Brave Browser
- Microsoft Edge
- Mozilla Firefox

---

## License
MIT License. Developed for research, performance, and interoperability purposes.
