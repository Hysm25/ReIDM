use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use reidm::categories::{determine_category, resolve_destination, resolve_unique_filename};
use reidm::types::{ChunkRecord, DownloadItem, DownloadStatus, Segment};
use std::io::Cursor;
use std::path::{Path, PathBuf};

#[test]
fn test_chunk_record_exact_16_byte_format() {
    // Verified from IDM registry [Software\DownloadManager\3\ChList]:
    // "0" = 00 00 00 00  02 00 00 00  00 00 00 00 00 00 00 00 (chunk 0, next 2, start 0)
    // "2" = 02 00 00 00  ff ff ff ff  8b e5 91 01 00 00 00 00 (chunk 2, next -1, start 26338699)
    let record0 = ChunkRecord::new(0, 2, 0);
    let bytes0 = record0.to_bytes();
    assert_eq!(
        bytes0,
        [0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );

    let decoded0 = ChunkRecord::from_bytes(&bytes0).unwrap();
    assert_eq!(decoded0, record0);

    let record2 = ChunkRecord::new(2, -1, 26338699);
    let bytes2 = record2.to_bytes();
    assert_eq!(
        bytes2,
        [0x02, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x8b, 0xe5, 0x91, 0x01, 0x00, 0x00, 0x00, 0x00]
    );

    let decoded2 = ChunkRecord::from_bytes(&bytes2).unwrap();
    assert_eq!(decoded2.chunk_id, 2);
    assert_eq!(decoded2.next_id, -1);
    assert_eq!(decoded2.start_pos, 26338699);
}

#[test]
fn test_dynamic_in_half_division_rule() {
    let temp_file1 = PathBuf::from("/tmp/test_chunk1");
    let temp_file2 = PathBuf::from("/tmp/test_chunk2");

    // Total file: 10,000,000 bytes [0 .. 9,999,999]
    let mut seg0 = Segment::new(0, 0, 9_999_999, -1, temp_file1);
    assert_eq!(seg0.remaining(), 10_000_000);

    // Split in half
    let seg1 = seg0.split_half(1, temp_file2).expect("Should split in half");

    // In half:
    // Half = 5,000,000.
    // seg0 should now cover [0 .. 4,999,999] with next_id = 1
    // seg1 should cover [5,000,000 .. 9,999,999] with next_id = -1
    assert_eq!(seg0.start_pos, 0);
    assert_eq!(seg0.end_pos, 4_999_999);
    assert_eq!(seg0.next_id, 1);
    assert_eq!(seg0.remaining(), 5_000_000);

    assert_eq!(seg1.id, 1);
    assert_eq!(seg1.start_pos, 5_000_000);
    assert_eq!(seg1.end_pos, 9_999_999);
    assert_eq!(seg1.next_id, -1);
    assert_eq!(seg1.remaining(), 5_000_000);
}

#[test]
fn test_idm_category_routing() {
    let base = Path::new("/home/user/Downloads");

    let (path_zip, cat_zip) = resolve_destination(base, "archive.7z");
    assert_eq!(cat_zip, "Compressed");
    assert_eq!(path_zip, base.join("Compressed"));

    let (path_video, cat_video) = resolve_destination(base, "movie.mp4");
    assert_eq!(cat_video, "Video");
    assert_eq!(path_video, base.join("Video"));

    let (path_doc, cat_doc) = resolve_destination(base, "manual.pdf");
    assert_eq!(cat_doc, "Documents");
    assert_eq!(path_doc, base.join("Documents"));

    let (path_prog, cat_prog) = resolve_destination(base, "setup.exe");
    assert_eq!(cat_prog, "Programs");
    assert_eq!(path_prog, base.join("Programs"));

    let (path_gen, cat_gen) = resolve_destination(base, "random.xyz");
    assert_eq!(cat_gen, "General");
    assert_eq!(path_gen, base.to_path_buf());
}

#[tokio::test]
async fn test_pause_and_resume_state_reconciliation() {
    let temp_root = std::env::temp_dir().join(format!("reidm_test_state_{}", std::process::id()));
    let state_manager = reidm::state::StateManager::new(temp_root.clone());
    let task_dir = state_manager.get_task_dir("test_file.bin", 12345);
    std::fs::create_dir_all(&task_dir).unwrap();

    let chunk0_path = task_dir.join("test_file.bin");
    let chunk1_path = task_dir.join("test_file.bin1");

    // Write partial bytes into chunk files
    std::fs::write(&chunk0_path, vec![0xAA; 1024]).unwrap();
    std::fs::write(&chunk1_path, vec![0xBB; 512]).unwrap();

    let mut item = DownloadItem {
        id: 12345,
        url: "http://example.com/test_file.bin".to_string(),
        file_name: "test_file.bin".to_string(),
        local_path: temp_root.join("downloads"),
        category: "General".to_string(),
        file_size: 4096,
        downloaded: 0, // initially un-reconciled
        status: DownloadStatus::Paused,
        resumable: true,
        user_agent: "ReIDM".to_string(),
        referer: None,
        cookies: None,
        etag: None,
        last_modified: None,
        content_type: None,
        created_at: chrono::Utc::now(),
        temp_dir: task_dir.clone(),
        segments: vec![
            Segment {
                id: 0,
                start_pos: 0,
                current_pos: 0,
                end_pos: 2047,
                downloaded: 0,
                next_id: 1,
                temp_file: chunk0_path.clone(),
                is_active: false,
            },
            Segment {
                id: 1,
                start_pos: 2048,
                current_pos: 2048,
                end_pos: 4095,
                downloaded: 0,
                next_id: -1,
                temp_file: chunk1_path.clone(),
                is_active: false,
            },
        ],
    };

    // Save state
    state_manager.save_state(&item).unwrap();

    // Verify chunks.bin is exact 32 bytes (2 x 16-byte records)
    let chunks_bin = task_dir.join("chunks.bin");
    assert!(chunks_bin.exists());
    assert_eq!(std::fs::metadata(&chunks_bin).unwrap().len(), 32);

    // Simulate extra bytes downloaded before sudden pause
    let mut f0 = std::fs::OpenOptions::new().append(true).open(&chunk0_path).unwrap();
    std::io::Write::write_all(&mut f0, &[0xCC; 256]).unwrap();
    drop(f0);

    // Reconcile / load state from disk
    let loaded = state_manager.load_state(&task_dir).unwrap();

    // Chunk 0 had 1024 + 256 = 1280 bytes
    assert_eq!(loaded.segments[0].downloaded, 1280);
    assert_eq!(loaded.segments[0].current_pos, 1280);
    assert_eq!(loaded.segments[0].is_active, false);

    // Chunk 1 had 512 bytes
    assert_eq!(loaded.segments[1].downloaded, 512);
    assert_eq!(loaded.segments[1].current_pos, 2048 + 512);
    assert_eq!(loaded.segments[1].is_active, false);

    // Total downloaded reconciled
    assert_eq!(loaded.downloaded, 1280 + 512);

    // Test find_resumable_task
    let found = state_manager.find_resumable_task("http://example.com/test_file.bin");
    assert_eq!(found, Some(task_dir.clone()));

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn test_queue_persistence_and_ordering() {
    let temp_root = std::env::temp_dir().join(format!("reidm_test_queue_{}", std::process::id()));
    let queue_file = temp_root.join("queue.json");

    let manager = reidm::scheduler::QueueManager::new(reidm::scheduler::QueueConfig::default());

    let item1 = DownloadItem {
        id: 1,
        url: "http://example.com/item1.zip".to_string(),
        file_name: "item1.zip".to_string(),
        local_path: temp_root.join("downloads"),
        category: "Compressed".to_string(),
        file_size: 1000,
        downloaded: 0,
        status: DownloadStatus::Idle,
        resumable: true,
        user_agent: "ReIDM".to_string(),
        referer: None,
        cookies: None,
        etag: None,
        last_modified: None,
        content_type: None,
        created_at: chrono::Utc::now(),
        temp_dir: temp_root.join("item1"),
        segments: vec![],
    };

    let item2 = DownloadItem {
        id: 2,
        url: "http://example.com/item2.mp4".to_string(),
        file_name: "item2.mp4".to_string(),
        local_path: temp_root.join("downloads"),
        category: "Video".to_string(),
        file_size: 2000,
        downloaded: 0,
        status: DownloadStatus::Idle,
        resumable: true,
        user_agent: "ReIDM".to_string(),
        referer: None,
        cookies: None,
        etag: None,
        last_modified: None,
        content_type: None,
        created_at: chrono::Utc::now(),
        temp_dir: temp_root.join("item2"),
        segments: vec![],
    };

    manager.add_and_save(item1.clone(), &queue_file).await.unwrap();
    manager.add_and_save(item2.clone(), &queue_file).await.unwrap();

    // Load in a fresh queue manager
    let loaded_manager = reidm::scheduler::QueueManager::load_from_file(&queue_file, reidm::scheduler::QueueConfig::default())
        .await
        .unwrap();

    assert_eq!(loaded_manager.pending_count().await, 2);

    // Start and pop
    loaded_manager.start();
    let popped1 = loaded_manager.pop_next().await.unwrap();
    assert_eq!(popped1.id, 1);
    assert_eq!(popped1.file_name, "item1.zip");

    // Max active is 1, so pop_next without completing should return None
    assert!(loaded_manager.pop_next().await.is_none());

    // Mark complete
    loaded_manager.mark_completed(popped1).await;

    // Now second item can be popped
    let popped2 = loaded_manager.pop_next().await.unwrap();
    assert_eq!(popped2.id, 2);
    assert_eq!(popped2.file_name, "item2.mp4");

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn test_assembly_linear_stitching() {
    let temp_root = std::env::temp_dir().join(format!("reidm_test_assemble_{}", std::process::id()));
    std::fs::create_dir_all(&temp_root).unwrap();

    let seg0_file = temp_root.join("chunk0");
    let seg1_file = temp_root.join("chunk1");
    let seg2_file = temp_root.join("chunk2");

    std::fs::write(&seg0_file, b"HELLO ").unwrap();
    std::fs::write(&seg1_file, b"WORLD ").unwrap();
    std::fs::write(&seg2_file, b"IDM!").unwrap();

    let segments = vec![
        // Intentionally provide out of order to verify sorting by start_pos
        Segment {
            id: 2,
            start_pos: 12,
            current_pos: 16,
            end_pos: 15,
            downloaded: 4,
            next_id: -1,
            temp_file: seg2_file,
            is_active: false,
        },
        Segment {
            id: 0,
            start_pos: 0,
            current_pos: 6,
            end_pos: 5,
            downloaded: 6,
            next_id: 1,
            temp_file: seg0_file,
            is_active: false,
        },
        Segment {
            id: 1,
            start_pos: 6,
            current_pos: 12,
            end_pos: 11,
            downloaded: 6,
            next_id: 2,
            temp_file: seg1_file,
            is_active: false,
        },
    ];

    let dest = temp_root.join("final_output.txt");
    let total = reidm::engine::assembler::Assembler::assemble_segments(&segments, &dest, 16).unwrap();

    assert_eq!(total, 16);
    let assembled_text = std::fs::read_to_string(&dest).unwrap();
    assert_eq!(assembled_text, "HELLO WORLD IDM!");

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_root);
}

