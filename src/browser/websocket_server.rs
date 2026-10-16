use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::accept_hdr_async;

#[derive(Debug, Clone)]
pub struct InterceptedDownload {
    pub url: String,
    pub referer: Option<String>,
    pub cookies: Option<String>,
    pub user_agent: Option<String>,
    pub filename: Option<String>,
}

pub struct ExtensionWebSocketServer {
    port: u16,
    download_tx: mpsc::Sender<InterceptedDownload>,
}

impl ExtensionWebSocketServer {
    pub fn new(port: u16, download_tx: mpsc::Sender<InterceptedDownload>) -> Self {
        Self { port, download_tx }
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr: SocketAddr = format!("127.0.0.1:{}", self.port).parse()?;
        let listener = TcpListener::bind(addr).await?;
        println!("IDM Extension WebSocket server listening on ws://{}", addr);

        loop {
            if let Ok((stream, _client_addr)) = listener.accept().await {
                let tx = self.download_tx.clone();
                tokio::spawn(async move {
                    Self::handle_connection(stream, tx).await;
                });
            }
        }
    }

    async fn handle_connection(stream: TcpStream, download_tx: mpsc::Sender<InterceptedDownload>) {
        // Accept subprotocol "plugin.v3.internetdownloadmanager.com" matching IDM extension
        let callback = |req: &Request, mut res: Response| {
            if let Some(subproto) = req.headers().get("Sec-WebSocket-Protocol") {
                if let Ok(proto_str) = subproto.to_str() {
                    if proto_str.contains("plugin.v3.internetdownloadmanager.com") {
                        res.headers_mut().insert(
                            "Sec-WebSocket-Protocol",
                            "plugin.v3.internetdownloadmanager.com".parse().unwrap(),
                        );
                    }
                }
            }
            Ok(res)
        };

        if let Ok(mut ws_stream) = accept_hdr_async(stream, callback).await {
            while let Some(msg_res) = ws_stream.next().await {
                match msg_res {
                    Ok(Message::Text(text)) => {
                        if let Some(download) = Self::parse_idm_message(&text) {
                            let _ = download_tx.send(download).await;
                            // Reply with takeover confirmation message
                            let reply = "MSG#1#1#1#0:OK";
                            let _ = ws_stream.send(Message::Text(reply.to_string())).await;
                        }
                    }
                    Ok(Message::Binary(bin)) => {
                        if let Ok(text) = String::from_utf8(bin) {
                            if let Some(download) = Self::parse_idm_message(&text) {
                                let _ = download_tx.send(download).await;
                                let reply = "MSG#1#1#1#0:OK";
                                let _ = ws_stream.send(Message::Text(reply.to_string())).await;
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    _ => {}
                }
            }
        }
    }

    /// Parses the IDM text protocol: MSG#seq#type#action#flags:params...,key=val...
    fn parse_idm_message(raw: &str) -> Option<InterceptedDownload> {
        // Format check: starts with MSG# or contains URL
        if raw.contains("http://") || raw.contains("https://") {
            // Find URL substring
            let mut url = None;
            let mut referer = None;
            let mut cookies = None;
            let mut user_agent = None;

            // Split into key=value tokens or parse tokens
            for part in raw.split(|c| c == ',' || c == ';' || c == '\n' || c == ':') {
                let trimmed = part.trim();
                if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                    if url.is_none() {
                        url = Some(trimmed.to_string());
                    } else if referer.is_none() {
                        referer = Some(trimmed.to_string());
                    }
                } else if trimmed.starts_with("Referer=") {
                    referer = Some(trimmed[8..].to_string());
                } else if trimmed.starts_with("Cookie=") {
                    cookies = Some(trimmed[7..].to_string());
                } else if trimmed.starts_with("User-Agent=") {
                    user_agent = Some(trimmed[11..].to_string());
                }
            }

            if let Some(u) = url {
                return Some(InterceptedDownload {
                    url: u,
                    referer,
                    cookies,
                    user_agent,
                    filename: None,
                });
            }
        }
        None
    }
}
