mod browser;
mod categories;
mod cli;
mod config;
mod engine;
mod scheduler;
mod state;
mod stream;
mod types;
mod ui;

use clap::Parser;
use cli::CliArgs;
use colored::*;
use config::Config;
use engine::{ConnectionManager, DynamicSegmenter, RateLimiter};
use state::StateManager;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;
use types::{DownloadItem, DownloadStatus};
use ui::TerminalProgress;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = CliArgs::parse();

    // 1. WebExtension Native Messaging Host mode
    if args.native_messaging {
        return run_native_messaging_host().await;
    }

    // 2. Install browser integration manifests
    if args.install_browser_integration {
        return install_browser_manifests();
    }

    // Load configuration and ensure directories
    let mut config = Config::default();
    if args.connections >= 1 && args.connections <= 32 {
        config.max_connections = args.connections;
    }
    if args.speed_limit_kb > 0 {
        config.speed_limit = Some(args.speed_limit_kb * 1024);
    }
    config.ensure_dirs()?;

    let conn_manager = Arc::new(ConnectionManager::new(
        config.user_agent.clone(),
        config.timeout_secs,
    )?);
    let state_manager = Arc::new(StateManager::new(config.temp_dir.clone()));
    let rate_limiter = Arc::new(RateLimiter::new(config.speed_limit));

    // 3. List paused/active tasks
    if args.list_tasks {
        let tasks = state_manager.list_paused_tasks();
        println!("{}", "════════════════════════════════════════════════════════════".cyan());
        println!("       {} - Saved / Paused Tasks", "ReIDM".bold().bright_green());
        println!("{}", "════════════════════════════════════════════════════════════".cyan());
        if tasks.is_empty() {
            println!("No paused tasks found.");
        } else {
            for (idx, task) in tasks.iter().enumerate() {
                println!(
                    "[{}] {} | {:.2}% ({} / {} bytes) | Segments: {}",
                    idx + 1,
                    task.file_name.bold().green(),
                    task.percent_complete(),
                    task.downloaded,
                    task.file_size,
                    task.segments.len()
                );
                println!("    Task Dir: {}", task.temp_dir.display());
                println!("    URL:      {}", task.url);
            }
        }
        return Ok(());
    }

    // 4. Background daemon mode for browser extension integration
    if args.daemon {
        return run_daemon_server(config, conn_manager, state_manager, rate_limiter).await;
    }

    // 5. Start queue (/s)
    let queue_file = config.state_dir.join("queue.json");
    if args.start_queue {
        println!("{}", "════════════════════════════════════════════════════════════".cyan());
        println!("       {} - Starting Download Queue", "ReIDM".bold().bright_green());
        println!("{}", "════════════════════════════════════════════════════════════".cyan());

        let queue = scheduler::QueueManager::load_from_file(&queue_file, scheduler::QueueConfig::default()).await?;
        queue.start();
        let total_queued = queue.pending_count().await;
        println!("Loaded {} pending items from queue.", total_queued);

        let config_arc = Arc::new(config.clone());
        while let Some(item) = queue.pop_next().await {
            println!("\n[Queue] Starting download: {}", item.file_name.bold().green());
            let item_clone = item.clone();
            let res = execute_download(
                item,
                config_arc.clone(),
                conn_manager.clone(),
                state_manager.clone(),
                rate_limiter.clone(),
                args.silent,
            )
            .await;

            if let Err(e) = res {
                eprintln!("[Queue] Download failed: {}", e);
            } else {
                queue.mark_completed(item_clone).await;
            }
            let _ = queue.save_to_file(&queue_file).await;
        }

        println!("\n{}", "Queue processing completed.".green().bold());
        return Ok(());
    }

    // 6. Resume specified task (-r)
    if let Some(ref resume_path) = args.resume {
        let task_dir = if resume_path.is_dir() {
            resume_path.clone()
        } else {
            config.temp_dir.join(resume_path)
        };

        if !task_dir.exists() {
            eprintln!("Error: Task directory does not exist: {:?}", task_dir);
            return Ok(());
        }

        println!("{}", "════════════════════════════════════════════════════════════".cyan());
        println!("       {} - Resuming Download Task", "ReIDM".bold().bright_green());
        println!("{}", "════════════════════════════════════════════════════════════".cyan());

        let item = state_manager.load_state(&task_dir)?;
        println!("Resuming:     {}", item.file_name.bold().green());
        println!("Downloaded:   {:.2}% ({} / {} bytes)", item.percent_complete(), item.downloaded, item.file_size);
        println!("Segments:     {}", item.segments.len());

        return execute_download(
            item,
            Arc::new(config),
            conn_manager,
            state_manager,
            rate_limiter,
            args.silent,
        )
        .await;
    }

    // 7. Command line download invocation (/d URL)
    if let Some(ref url) = args.url {
        // Check if an existing paused task exists for this URL
        if let Some(task_dir) = state_manager.find_resumable_task(url) {
            println!("{}", "Found existing paused task for this URL. Resuming automatically...".yellow());
            let item = state_manager.load_state(&task_dir)?;
            return execute_download(
                item,
                Arc::new(config),
                conn_manager,
                state_manager,
                rate_limiter,
                args.silent,
            )
            .await;
        }

        println!("{}", "════════════════════════════════════════════════════════════".cyan());
        println!("       {} - Internet Download Manager (Rust)", "ReIDM".bold().bright_green());
        println!("{}", "════════════════════════════════════════════════════════════".cyan());
        println!("Target URL: {}", url.white().bold());

        // Probe the URL
        print!("Probing server capabilities... ");
        let probe = conn_manager
            .probe(url, args.referer.as_deref(), args.cookie.as_deref())
            .await?;
        println!("{}", "OK".green().bold());

        let file_name = args.local_file_name.unwrap_or(probe.file_name);
        let (dest_dir, category) = if let Some(p) = args.local_path {
            (p, "Custom".to_string())
        } else {
            categories::resolve_destination(&config.download_dir, &file_name)
        };

        let unique_filename = categories::resolve_unique_filename(&dest_dir, &file_name);
        let download_id = chrono::Utc::now().timestamp_millis() as u64;
        let task_dir = state_manager.get_task_dir(&unique_filename, download_id);

        println!("File Name:    {}", unique_filename.green().bold());
        println!("Category:     {}", category.yellow());
        println!("Destination:  {}", dest_dir.display().to_string().cyan());
        println!(
            "File Size:    {}",
            if probe.file_size > 0 {
                format!("{:.2} MB ({} bytes)", probe.file_size as f64 / 1_048_576.0, probe.file_size)
            } else {
                "Unknown (streaming)".to_string()
            }
        );
        println!(
            "Resumable:    {}",
            if probe.resumable {
                "Yes (Accept-Ranges supported)".green()
            } else {
                "No (single-stream mode)".red()
            }
        );
        println!("Connections:  {}", config.max_connections.to_string().magenta());

        let item = DownloadItem {
            id: download_id,
            url: url.clone(),
            file_name: unique_filename.clone(),
            local_path: dest_dir.clone(),
            category,
            file_size: probe.file_size,
            downloaded: 0,
            status: DownloadStatus::Idle,
            resumable: probe.resumable,
            user_agent: config.user_agent.clone(),
            referer: args.referer,
            cookies: args.cookie,
            etag: probe.etag,
            last_modified: probe.last_modified,
            content_type: probe.content_type.clone(),
            created_at: chrono::Utc::now(),
            temp_dir: task_dir,
            segments: Vec::new(),
        };

        if args.add_only {
            let queue = scheduler::QueueManager::load_from_file(&queue_file, scheduler::QueueConfig::default()).await?;
            queue.add_and_save(item, &queue_file).await?;
            println!("{}", "Download successfully added to queue (/a specified).".green().bold());
            return Ok(());
        }

        // Check if HLS M3U8 stream
        if url.contains(".m3u8") || probe.content_type.as_deref() == Some("application/x-mpegURL") {
            println!("{}", "Detected HLS M3U8 stream. Starting stream extraction...".yellow());
            let m3u8_content = conn_manager.client().get(url).send().await?.text().await?;
            let segments = stream::M3u8Parser::parse(&m3u8_content, url)?;
            let final_dest = dest_dir.join(&unique_filename);
            let written = stream::M3u8Parser::download_hls(conn_manager.client(), &segments, &final_dest).await?;
            println!("HLS Stream downloaded: {} bytes to {:?}", written, final_dest);
            return Ok(());
        }

        return execute_download(
            item,
            Arc::new(config),
            conn_manager,
            state_manager,
            rate_limiter,
            args.silent,
        )
        .await;
    }

    println!("ReIDM: No download URL or queue action provided. Run `reidm --help` for usage.");
    Ok(())
}

/// Executes a download with real-time UI and graceful Ctrl+C interruption
async fn execute_download(
    mut item: DownloadItem,
    config: Arc<Config>,
    conn_manager: Arc<ConnectionManager>,
    state_manager: Arc<StateManager>,
    rate_limiter: Arc<RateLimiter>,
    silent: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (segmenter, mut progress_rx) = DynamicSegmenter::new(
        config.clone(),
        conn_manager.clone(),
        state_manager.clone(),
        rate_limiter.clone(),
    );

    let progress_handle = tokio::spawn(async move {
        while let Ok(update) = progress_rx.recv().await {
            if !silent {
                TerminalProgress::render(&update, 50);
            }
        }
    });

    let stop_signal = Arc::new(AtomicBool::new(false));
    let stop_clone = stop_signal.clone();

    // Graceful Ctrl+C handler
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            println!("\n{}", "[ReIDM] Interrupt received. Pausing download and saving chunk state...".yellow().bold());
            stop_clone.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });

    segmenter.download(&mut item, stop_signal).await?;
    progress_handle.abort();

    if !silent {
        TerminalProgress::finish();
    }

    if item.status == DownloadStatus::Complete {
        println!("Status:       {}", "Complete".green().bold());
        println!("Saved to:     {}", item.final_destination().display().to_string().green().bold());
    } else if item.status == DownloadStatus::Paused {
        println!("Status:       {}", "Paused".yellow().bold());
        println!("Task State:   {}", item.temp_dir.display().to_string().cyan());
        println!("Resume with:  reidm -r {}", item.temp_dir.display());
    }

    Ok(())
}

/// Installs WebExtension Native Messaging Host manifests into browser configuration directories
fn install_browser_manifests() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let current_exe = std::env::current_exe()?;
    let home = std::env::var("HOME")?;
    let home_path = std::path::Path::new(&home);

    // Chrome / Chromium manifest
    let chrome_manifest = serde_json::json!({
        "name": "com.tonec.idm",
        "description": "Internet Download Manager extension helper",
        "path": current_exe.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [
            "chrome-extension://ngpampappnmepgilojfohadhhmbhlaek/",
            "chrome-extension://llbjbkhnmlidjebalopleeepgdfgcpec/"
        ]
    });

    // Firefox manifest
    let mozilla_manifest = serde_json::json!({
        "name": "com.tonec.idm",
        "description": "Internet Download Manager extension helper",
        "path": current_exe.to_string_lossy(),
        "type": "stdio",
        "allowed_extensions": [
            "mozilla_cc3@internetdownloadmanager.com"
        ]
    });

    let chrome_dirs = [
        home_path.join(".config/google-chrome/NativeMessagingHosts"),
        home_path.join(".config/chromium/NativeMessagingHosts"),
        home_path.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts"),
        home_path.join(".config/microsoft-edge/NativeMessagingHosts"),
    ];

    for dir in chrome_dirs {
        std::fs::create_dir_all(&dir)?;
        let manifest_file = dir.join("com.tonec.idm.json");
        std::fs::write(&manifest_file, serde_json::to_string_pretty(&chrome_manifest)?)?;
        println!("Installed Chrome manifest: {}", manifest_file.display());
    }

    let mozilla_dir = home_path.join(".mozilla/native-messaging-hosts");
    std::fs::create_dir_all(&mozilla_dir)?;
    let mozilla_file = mozilla_dir.join("com.tonec.idm.json");
    std::fs::write(&mozilla_file, serde_json::to_string_pretty(&mozilla_manifest)?)?;
    println!("Installed Mozilla manifest: {}", mozilla_file.display());

    println!("{}", "Native Messaging integration successfully installed for all browsers!".green().bold());
    Ok(())
}

/// Runs WebExtension Native Messaging Host
async fn run_native_messaging_host() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use browser::NativeMessagingHost;
    while let Ok(Some(msg)) = NativeMessagingHost::read_message() {
        if let Some(url) = msg.url {
            let resp = browser::NativeMessageOut {
                cancel: true,
                takeover: true,
                message: format!("ReIDM taken over download: {}", url),
            };
            let _ = NativeMessagingHost::send_response(&resp);
        }
    }
    Ok(())
}

/// Runs WebSocket background server for browser extension integration on ws://127.0.0.1:1001
async fn run_daemon_server(
    config: Config,
    conn_manager: Arc<ConnectionManager>,
    state_manager: Arc<StateManager>,
    rate_limiter: Arc<RateLimiter>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (tx, mut rx) = mpsc::channel(32);
    let server = browser::ExtensionWebSocketServer::new(config.ws_port, tx);

    tokio::spawn(async move {
        if let Err(e) = server.run().await {
            eprintln!("Extension WebSocket server error: {}", e);
        }
    });

    println!("ReIDM daemon started. Awaiting downloads from browser extensions...");

    while let Some(download) = rx.recv().await {
        println!("Intercepted download from browser extension: {}", download.url);
        let cfg = config.clone();
        let cm = conn_manager.clone();
        let sm = state_manager.clone();
        let rl = rate_limiter.clone();

        tokio::spawn(async move {
            if let Ok(probe) = cm.probe(&download.url, download.referer.as_deref(), download.cookies.as_deref()).await {
                let (dest_dir, category) = categories::resolve_destination(&cfg.download_dir, &probe.file_name);
                let unique_filename = categories::resolve_unique_filename(&dest_dir, &probe.file_name);
                let download_id = chrono::Utc::now().timestamp_millis() as u64;
                let task_dir = sm.get_task_dir(&unique_filename, download_id);

                let mut item = DownloadItem {
                    id: download_id,
                    url: download.url,
                    file_name: unique_filename,
                    local_path: dest_dir,
                    category,
                    file_size: probe.file_size,
                    downloaded: 0,
                    status: DownloadStatus::Idle,
                    resumable: probe.resumable,
                    user_agent: download.user_agent.unwrap_or(cfg.user_agent.clone()),
                    referer: download.referer,
                    cookies: download.cookies,
                    etag: probe.etag,
                    last_modified: probe.last_modified,
                    content_type: probe.content_type,
                    created_at: chrono::Utc::now(),
                    temp_dir: task_dir,
                    segments: Vec::new(),
                };

                let (segmenter, _) = DynamicSegmenter::new(Arc::new(cfg), cm, sm, rl);
                let stop = Arc::new(AtomicBool::new(false));
                let _ = segmenter.download(&mut item, stop).await;
            }
        });
    }

    Ok(())
}
