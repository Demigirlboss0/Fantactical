use std::time::Duration;
use std::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};

use fantactical::network::{ClientMessage, ServerMessage};
use fantactical::server::{spawn_server, ServerConfig, ServerOutgoing, ServerIncoming};
use fantactical::client::{spawn_client, ClientOutgoing, ClientIncoming};

fn find_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .map(|l| l.local_addr().unwrap().port())
        .unwrap_or(19000)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_server_client_auth_and_broadcast() {
    let port = find_free_port();
    let server_config = ServerConfig {
        port,
        session_token: "integration-test".into(),
    };

    let (outgoing_tx, mut incoming_rx) = spawn_server(server_config);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let url = format!("ws://127.0.0.1:{}/", port);
    let result = connect_async(&url).await;
    let (ws_stream, _) = result.expect("connect");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let auth = ClientMessage::Auth { token: "integration-test".into() };
    let json = serde_json::to_string(&auth).unwrap();
    ws_sender.send(Message::Text(json)).await.unwrap();

    let next = ws_receiver.next().await;
    if let Some(Ok(Message::Text(text))) = next {
        let msg: ServerMessage = serde_json::from_str(&text).unwrap();
        assert!(matches!(msg, ServerMessage::AuthSuccess { .. }),
            "Expected AuthSuccess, got {:?}", msg);
    } else {
        panic!("No AuthSuccess received: {:?}", next);
    }

    let msg = incoming_rx.recv().await.expect("ClientConnected");
    assert!(matches!(msg, ServerIncoming::ClientConnected { .. }));

    let msg = incoming_rx.recv().await.expect("Auth forwarded");
    assert!(matches!(msg, ServerIncoming::Message {
        message: ClientMessage::Auth { .. }, ..
    }));

    let cmd = ClientMessage::DeclareManeuver {
        source_id: 1,
        target_id: Some(2),
        target_hex: None,
        maneuver: fantactical::model::ManeuverType::Attack,
        extra_efforts: vec![],
    };
    let json = serde_json::to_string(&cmd).unwrap();
    ws_sender.send(Message::Text(json)).await.unwrap();

    let msg = incoming_rx.recv().await.expect("maneuver message");
                match msg {
        ServerIncoming::Message { client_id: _, message } => {
            assert!(matches!(message, ClientMessage::DeclareManeuver { .. }));
        }
        other => panic!("Expected Message, got {:?}", other),
    }

    let broadcast = ServerMessage::RollResult { label: "Roll: 10".into(), roll: 10 };
    outgoing_tx.send(ServerOutgoing::Broadcast(broadcast)).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    if let Some(Ok(Message::Text(text))) = ws_receiver.next().await {
        let msg: ServerMessage = serde_json::from_str(&text).unwrap();
        assert!(matches!(msg, ServerMessage::RollResult { .. }),
            "Expected RollResult, got {:?}", msg);
    }

    drop(ws_sender);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_server_multi_client_second_is_non_gm() {
    let port = find_free_port();
    let server_config = ServerConfig {
        port,
        session_token: "multi-test".into(),
    };

    let (_outgoing_tx, _incoming_rx) = spawn_server(server_config);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let url = format!("ws://127.0.0.1:{}/", port);

    let (ws1, _) = connect_async(&url).await.unwrap();
    let (mut s1, mut r1) = ws1.split();
    let auth = ClientMessage::Auth { token: "multi-test".into() };
    s1.send(Message::Text(serde_json::to_string(&auth).unwrap())).await.unwrap();

    if let Some(Ok(Message::Text(text))) = r1.next().await {
        let msg: ServerMessage = serde_json::from_str(&text).unwrap();
        assert!(matches!(msg, ServerMessage::AuthSuccess { is_gm: true, .. }),
            "First client should be GM");
    }

    let (ws2, _) = connect_async(&url).await.unwrap();
    let (mut s2, mut r2) = ws2.split();
    s2.send(Message::Text(serde_json::to_string(&auth).unwrap())).await.unwrap();

    if let Some(Ok(Message::Text(text))) = r2.next().await {
        let msg: ServerMessage = serde_json::from_str(&text).unwrap();
        assert!(matches!(msg, ServerMessage::AuthSuccess { is_gm: false, .. }),
            "Second client should NOT be GM, got {:?}", msg);
    }
}

#[tokio::test]
async fn test_client_connect_to_dead_server() {
    let (outgoing_tx, mut incoming_rx) = spawn_client();

    let dead_port = find_free_port();
    outgoing_tx.send(ClientOutgoing::Connect {
        host: "127.0.0.1".into(),
        port: dead_port,
        token: "test".into(),
    }).unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(5), incoming_rx.recv())
        .await
        .expect("timeout")
        .expect("message");
    assert!(matches!(msg, ClientIncoming::Disconnected { .. } | ClientIncoming::Status(_)),
        "Expected Disconnected or Status, got {:?}", msg);

    let _ = outgoing_tx.send(ClientOutgoing::Disconnect);
}
