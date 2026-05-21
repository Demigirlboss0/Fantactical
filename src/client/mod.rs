use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use log;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;

use crate::network::{ClientMessage, ServerMessage};

#[derive(Debug, Clone)]
pub enum ClientOutgoing {
    Connect {
        host: String,
        port: u16,
        token: String,
    },
    Disconnect,
    Send(ClientMessage),
}

#[derive(Debug, Clone)]
pub enum ClientIncoming {
    Connected { client_id: u64, is_gm: bool },
    Disconnected { reason: String },
    Message(ServerMessage),
    Status(String),
}

pub fn spawn_client() -> (
    mpsc::UnboundedSender<ClientOutgoing>,
    mpsc::UnboundedReceiver<ClientIncoming>,
) {
    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<ClientOutgoing>();
    let (incoming_tx, incoming_rx) = mpsc::unbounded_channel::<ClientIncoming>();

    let incoming = incoming_tx.clone();

    tokio::spawn(async move {
        let mut sender: Option<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>> =
            None;
        let mut backoff_secs: u64 = 1;

        loop {
            tokio::select! {
                Some(outgoing) = outgoing_rx.recv() => {
                    match outgoing {
                        ClientOutgoing::Connect { host, port, token } => {
                            let _ = incoming.send(ClientIncoming::Status(format!("Connecting to {}:{}...", host, port)));
                            let url = format!("ws://{}:{}/", host, port);
                            match connect_async(&url).await {
                                Ok((ws_stream, _)) => {
                                    let _ = incoming.send(ClientIncoming::Status("Connected, authenticating...".into()));
                                    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

                                    let auth = ClientMessage::Auth { token };
                                    if let Ok(json) = serde_json::to_string(&auth) {
                                        let _ = ws_sender.send(Message::Text(json)).await;
                                    }

                                    // Read loop — process server messages
                                    loop {
                                        tokio::select! {
                                            result = ws_receiver.next() => {
                                                match result {
                                                    Some(Ok(Message::Text(text))) => {
                                                        match serde_json::from_str::<ServerMessage>(&text) {
                                                            Ok(ServerMessage::AuthSuccess { client_id, is_gm }) => {
                                                                let _ = incoming.send(ClientIncoming::Connected { client_id, is_gm });
                                                                log::info!("Client authenticated as id={}, gm={}", client_id, is_gm);
                                                                backoff_secs = 1;
                                                            }
                                                            Ok(other) => {
                                                                let _ = incoming.send(ClientIncoming::Message(other));
                                                            }
                                                            Err(e) => {
                                                                log::warn!("Failed to parse: {e}");
                                                            }
                                                        }
                                                    }
                                                    Some(Ok(Message::Close(_))) | None => {
                                                        let _ = incoming.send(ClientIncoming::Disconnected {
                                                            reason: "Connection closed".into()
                                                        });
                                                        break;
                                                    }
                                                    Some(Ok(Message::Ping(data))) => {
                                                        let _ = ws_sender.send(Message::Pong(data)).await;
                                                    }
                                                    Some(Err(e)) => {
                                                        let _ = incoming.send(ClientIncoming::Disconnected {
                                                            reason: format!("Error: {e}")
                                                        });
                                                        break;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                            Some(outgoing) = outgoing_rx.recv() => {
                                                match outgoing {
                                                    ClientOutgoing::Send(cm) => {
                                                        if let Ok(json) = serde_json::to_string(&cm) {
                                                            let _ = ws_sender.send(Message::Text(json)).await;
                                                        }
                                                    }
                                                    ClientOutgoing::Disconnect => {
                                                        let _ = ws_sender.close().await;
                                                        break;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }

                                    sender = None;
                                }
                                Err(e) => {
                                    let _ = incoming.send(ClientIncoming::Disconnected {
                                        reason: format!("Connect failed: {e}")
                                    });
                                }
                            }
                        }
                        ClientOutgoing::Disconnect => {
                            if let Some(ref mut s) = sender {
                                let _ = s.close().await;
                            }
                            sender = None;
                        }
                        ClientOutgoing::Send(_) => {
                            // ignored if not connected
                        }
                    }
                }
            }

            // If we get here, connection was lost — try reconnect
            let _ = incoming.send(ClientIncoming::Status(format!(
                "Reconnecting in {}s...",
                backoff_secs
            )));
            tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(30);
        }
    });

    (outgoing_tx, incoming_rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_client_creates_channels() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (outgoing_tx, mut incoming_rx) = spawn_client();
            // Connect to a dead port — client should emit Status then Disconnected
            let send_result = outgoing_tx.send(ClientOutgoing::Connect {
                host: "127.0.0.1".into(),
                port: 9999,
                token: "test".into(),
            });
            assert!(send_result.is_ok(), "channel should accept connect request");
            // Read the first incoming message — should be Status("Connecting to ...")
            match incoming_rx.recv().await {
                Some(msg) => {
                    assert!(
                        matches!(msg, ClientIncoming::Status(_) | ClientIncoming::Disconnected { .. }),
                        "expected Status or Disconnected, got {:?}", msg
                    );
                }
                None => panic!("incoming channel closed unexpectedly"),
            }
        });
    }

    #[test]
    fn test_client_outgoing_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ClientOutgoing>();
        assert_send_sync::<ClientIncoming>();
    }

    #[test]
    fn test_disconnect_before_connect() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let (outgoing_tx, _incoming_rx) = spawn_client();
            // Send disconnect before any connect — the client task should handle it
            let send_result = outgoing_tx.send(ClientOutgoing::Disconnect);
            assert!(send_result.is_ok(), "channel should accept disconnect");
            // The client task should survive a disconnect sent before connect.
            // Verify by sending another message after a brief pause.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let second_send = outgoing_tx.send(ClientOutgoing::Disconnect);
            assert!(
                second_send.is_ok(),
                "channel should still be open after pre-connect disconnect"
            );
        });
    }
}
