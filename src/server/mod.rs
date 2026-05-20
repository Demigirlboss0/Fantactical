use log;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt}; 

use crate::network::{ClientMessage, ServerMessage, DEFAULT_PORT};

pub type ClientId = u64;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub session_token: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            session_token: "fantactical".to_string(),
        }
    }
}

struct ClientState {
    #[allow(dead_code)]
    client_id: ClientId,
    #[allow(dead_code)]
    is_gm: bool,
    sender: mpsc::UnboundedSender<Message>,
    #[allow(dead_code)]
    assigned_actors: Vec<crate::model::ActorId>,
}

struct ServerState {
    clients: HashMap<ClientId, ClientState>,
    next_client_id: ClientId,
    #[allow(dead_code)]
    config: ServerConfig,
}

/// Channel for Bevy → server messages (outgoing to clients)
#[derive(Debug, Clone)]
pub enum ServerOutgoing {
    Broadcast(ServerMessage),
    SendTo(ClientId, ServerMessage),
    SetActorOwnership { client_id: ClientId, actor_ids: Vec<crate::model::ActorId> },
}

/// Channel for server → Bevy messages (incoming from clients)
#[derive(Debug, Clone)]
pub enum ServerIncoming {
    ClientConnected { client_id: ClientId, is_gm: bool },
    ClientDisconnected { client_id: ClientId },
    Message { client_id: ClientId, message: ClientMessage },
}

/// Spawn the WebSocket server in a background tokio task.
/// Returns channels for communicating with the Bevy app.
pub fn spawn_server(
    #[allow(dead_code)]
    config: ServerConfig,
) -> (
    mpsc::UnboundedSender<ServerOutgoing>,
    mpsc::UnboundedReceiver<ServerIncoming>,
) {
    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<ServerOutgoing>();
    let (incoming_tx, incoming_rx) = mpsc::unbounded_channel::<ServerIncoming>();

    let state = Arc::new(Mutex::new(ServerState {
        clients: HashMap::new(),
        next_client_id: 1,
        config: config.clone(),
    }));

    let incoming_for_task = incoming_tx.clone();
    let state_for_accept = state.clone();

    tokio::spawn(async move {
        let addr = format!("0.0.0.0:{}", config.port);
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => {
                log::info!("WebSocket server listening on {}", addr);
                l
            }
            Err(e) => {
                log::error!("Failed to bind server on {}: {}", addr, e);
                return;
            }
        };

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, peer_addr)) => {
                            log::info!("New connection from {}", peer_addr);
                            let state = state_for_accept.clone();
                            let incoming = incoming_for_task.clone();
                            tokio::spawn(handle_connection(stream, state, incoming));
                        }
                        Err(e) => {
                            log::error!("Accept error: {}", e);
                        }
                    }
                }
                Some(outgoing) = outgoing_rx.recv() => {
                    let state = state.lock().await;
                    match outgoing {
                        ServerOutgoing::Broadcast(msg) => {
                            let json = match serde_json::to_string(&msg) {
                                Ok(j) => j,
                                Err(e) => { log::error!("Serialize error: {e}"); continue; }
                            };
                            for client in state.clients.values() {
                                let _ = client.sender.send(Message::Text(json.clone()));
                            }
                        }
                        ServerOutgoing::SendTo(client_id, msg) => {
                            let json = match serde_json::to_string(&msg) {
                                Ok(j) => j,
                                Err(e) => { log::error!("Serialize error: {e}"); continue; }
                            };
                            if let Some(client) = state.clients.get(&client_id) {
                                let _ = client.sender.send(Message::Text(json));
                            }
                        }
                        ServerOutgoing::SetActorOwnership { client_id, actor_ids } => {
                            // ownership set via incoming separately
                            let _ = (client_id, actor_ids);
                        }
                    }
                }
            }
        }
    });

    (outgoing_tx, incoming_rx)
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    state: Arc<Mutex<ServerState>>,
    incoming: mpsc::UnboundedSender<ServerIncoming>,
) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            log::error!("WebSocket handshake failed: {e}");
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // --- Authentication handshake ---
    // Read the first message; it must be a valid Auth message
    let (client_id, is_gm, client_tx, mut client_rx) = {
        let valid_token = {
            let guard = state.lock().await;
            guard.config.session_token.clone()
        };

        match ws_receiver.next().await {
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Auth { token }) => {
                        if token != valid_token {
                            let fail = ServerMessage::AuthFailure {
                                reason: "Invalid session token".into()
                            };
                            if let Ok(json) = serde_json::to_string(&fail) {
                                let _ = ws_sender.send(Message::Text(json)).await;
                            }
                            log::warn!("Client rejected: bad token");
                            return;
                        }
                    }
                    _ => {
                        let fail = ServerMessage::AuthFailure {
                            reason: "Expected Auth message".into()
                        };
                        if let Ok(json) = serde_json::to_string(&fail) {
                            let _ = ws_sender.send(Message::Text(json)).await;
                        }
                        log::warn!("Client rejected: no Auth message");
                        return;
                    }
                }
            }
            _ => {
                log::warn!("Client disconnected before auth");
                return;
            }
        }

        let mut guard = state.lock().await;
        let cid = guard.next_client_id;
        guard.next_client_id += 1;
        let gm = guard.clients.is_empty(); // first client is GM
        let (tx, rx) = mpsc::unbounded_channel::<Message>();
        guard.clients.insert(cid, ClientState {
            client_id: cid,
            is_gm: gm,
            sender: tx.clone(),
            assigned_actors: Vec::new(),
        });
        (cid, gm, tx, rx)
    };

    let _ = incoming.send(ServerIncoming::ClientConnected { client_id, is_gm });
    let _ = incoming.send(ServerIncoming::Message {
        client_id,
        message: ClientMessage::Auth { token: String::new() },
    });

    // Send auth success AFTER validation
    let auth_msg = ServerMessage::AuthSuccess { client_id, is_gm };
    if let Ok(json) = serde_json::to_string(&auth_msg) {
        let _ = ws_sender.send(Message::Text(json)).await;
    }

    // Read incoming messages and forward outgoing via select!
    loop {
        tokio::select! {
            result = ws_receiver.next() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(cm) => {
                                let _ = incoming.send(ServerIncoming::Message { client_id, message: cm });
                            }
                            Err(e) => {
                                log::warn!("Failed to parse client message: {e}");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_sender.send(Message::Pong(data)).await;
                    }
                    Some(Err(e)) => {
                        log::warn!("WebSocket error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
            Some(msg) = client_rx.recv() => {
                if ws_sender.send(msg).await.is_err() {
                    break;
                }
            }
        }
    }

    {
        let mut state_guard = state.lock().await;
        state_guard.clients.remove(&client_id);
    }

    let _ = incoming.send(ServerIncoming::ClientDisconnected { client_id });
    log::info!("Client {} disconnected", client_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_defaults() {
        let config = ServerConfig::default();
        assert_eq!(config.port, DEFAULT_PORT);
        assert_eq!(config.session_token, "fantactical");
    }

    #[test]
    fn test_server_config_custom() {
        let config = ServerConfig {
            port: 1234,
            session_token: "my-token".into(),
        };
        assert_eq!(config.port, 1234);
        assert_eq!(config.session_token, "my-token");
    }

    #[test]
    fn test_spawn_server_creates_channels() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let config = ServerConfig::default();
        let (outgoing_tx, _incoming_rx) = spawn_server(config);
        // Verify channels work by sending a message
        let result = outgoing_tx.send(ServerOutgoing::Broadcast(
            crate::network::ServerMessage::Error { message: "test".into() }
        ));
        assert!(result.is_ok());
    }

    #[test]
    fn test_server_outgoing_variants_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ServerOutgoing>();
        assert_send_sync::<ServerIncoming>();
    }
}
