use clap::Parser;
use std::path::PathBuf;

/// ReIDM: High-Performance Internet Download Manager rewritten in Rust
#[derive(Parser, Debug)]
#[command(name = "reidm", version = "0.1.0", author = "ReIDM Contributors")]
pub struct CliArgs {
    /// Download URL (/d in IDM)
    #[arg(short = 'd', long = "url")]
    pub url: Option<String>,

    /// Save path directory (/p in IDM)
    #[arg(short = 'p', long = "path")]
    pub local_path: Option<PathBuf>,

    /// Target file name (/f in IDM)
    #[arg(short = 'f', long = "filename")]
    pub local_file_name: Option<String>,

    /// Add file to download queue but do not start downloading immediately (/a in IDM)
    #[arg(short = 'a', long = "add-only")]
    pub add_only: bool,

    /// Start download queue in scheduler (/s in IDM)
    #[arg(short = 's', long = "start-queue")]
    pub start_queue: bool,

    /// Silent mode: no interactive prompts (/n in IDM)
    #[arg(short = 'n', long = "silent")]
    pub silent: bool,

    /// Exit after successful downloading (/q in IDM)
    #[arg(short = 'q', long = "quit")]
    pub quit: bool,

    /// Maximum concurrent connections per download (1..32, default: 8)
    #[arg(short = 'c', long = "connections", default_value_t = 8)]
    pub connections: usize,

    /// Speed limit in kilobytes per second (0 = unlimited)
    #[arg(short = 'l', long = "limit", default_value_t = 0)]
    pub speed_limit_kb: u64,

    /// Launch as WebExtension Native Messaging Host (com.tonec.idm stdio protocol)
    #[arg(long = "native-messaging")]
    pub native_messaging: bool,

    /// Run daemon background server with WebSocket interception on ws://127.0.0.1:1001
    #[arg(long = "daemon")]
    pub daemon: bool,

    /// Custom referer header
    #[arg(long = "referer")]
    pub referer: Option<String>,

    /// Custom cookie header
    #[arg(long = "cookie")]
    pub cookie: Option<String>,

    /// Resume an interrupted or paused download task directory or ID (-r)
    #[arg(short = 'r', long = "resume")]
    pub resume: Option<PathBuf>,

    /// List all paused or saved download tasks
    #[arg(long = "list-tasks")]
    pub list_tasks: bool,

    /// Install native messaging host manifests into Chrome, Edge, and Firefox configuration paths
    #[arg(long = "install-browser-integration")]
    pub install_browser_integration: bool,

    /// Launch graphical user interface
    #[arg(long = "gui")]
    pub gui: bool,
}
