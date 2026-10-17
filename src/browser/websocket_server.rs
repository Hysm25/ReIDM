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
        use super::protocol::IdmMessage;

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
                let text_data = match msg_res {
                    Ok(Message::Text(t)) => Some(t),
                    Ok(Message::Binary(bin)) => String::from_utf8(bin).ok(),
                    Ok(Message::Close(_)) => break,
                    _ => None,
                };

                if let Some(raw) = text_data {
                    if let Some(idm_msg) = IdmMessage::parse(&raw) {
                        match idm_msg.msg_type {
                            // Handshake (Type 2, Subaction 5)
                            2 => {
                                let resp1 = IdmMessage::handshake_response(idm_msg.seq);
                                let _ = ws_stream.send(Message::Text(resp1)).await;
                                let resp2 = IdmMessage::config_update(idm_msg.seq + 1);
                                let _ = ws_stream.send(Message::Text(resp2)).await;
                            }
                            // Heartbeat / Keepalive Ping (Type 0)
                            0 => {
                                let ping_ack = IdmMessage::ping_ack(idm_msg.seq);
                                let _ = ws_stream.send(Message::Text(ping_ack)).await;
                            }
                            // Download Interception from chrome.downloads (Type 13)
                            13 => {
                                if let Some(download) = Self::extract_from_idm_msg(&idm_msg) {
                                    let _ = download_tx.send(download).await;
                                    let dl_id = idm_msg.positional_args.first().map(|s| s.as_str()).unwrap_or("0");
                                    let takeover = IdmMessage::download_takeover(idm_msg.seq, dl_id);
                                    let _ = ws_stream.send(Message::Text(takeover)).await;
                                }
                            }
                            // Direct Context Menu / Click Download (Type 14)
                            14 => {
                                if let Some(download) = Self::extract_from_idm_msg(&idm_msg) {
                                    let _ = download_tx.send(download).await;
                                    let ack = IdmMessage::ping_ack(idm_msg.seq);
                                    let _ = ws_stream.send(Message::Text(ack)).await;
                                }
                            }
                            _ => {
                                // Fallback ack
                                let ack = IdmMessage::ping_ack(idm_msg.seq);
                                let _ = ws_stream.send(Message::Text(ack)).await;
                            }
                        }
                    } else if let Some(download) = Self::parse_fallback(&raw) {
                        let _ = download_tx.send(download).await;
                        let reply = "MSG#1#1#1#0:OK;";
                        let _ = ws_stream.send(Message::Text(reply.to_string())).await;
                    }
                }
            }
        }
    }

    /// Extracts download parameters from a parsed IDM extension message
    fn extract_from_idm_msg(msg: &super::protocol::IdmMessage) -> Option<InterceptedDownload> {
        let url = msg.key_values.get(&6).cloned().or_else(|| msg.key_values.get(&17).cloned())?;
        let referer = msg.key_values.get(&50).cloned();
        let cookies = msg.key_values.get(&51).cloned();
        let user_agent = msg.key_values.get(&54).cloned();
        let filename = msg.key_values.get(&100).cloned().or_else(|| msg.key_values.get(&4).cloned());

        Some(InterceptedDownload {
            url,
            referer,
            cookies,
            user_agent,
            filename,
        })
    }

    /// Legacy fallback parser for non-conforming messages
    fn parse_fallback(raw: &str) -> Option<InterceptedDownload> {
        if raw.contains("http://") || raw.contains("https://") {
            let mut url = None;
            let mut referer = None;
            let mut cookies = None;
            let mut user_agent = None;

            for part in raw.split(|c| c == ',' || c == ';' || c == '\n') {
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
