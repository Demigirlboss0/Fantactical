use bevy::prelude::*;
use tokio::sync::mpsc;

use crate::client::{spawn_client, ClientIncoming, ClientOutgoing};
use crate::network::ServerMessage;
use crate::server::{ServerIncoming, ServerOutgoing, ServerConfig};
use crate::ui::battlemap::GameStateResource;

pub struct NetworkPlugin {
    pub mode: NetworkMode,
    pub session_token: String,
    pub connect_host: Option<String>,
    pub connect_port: Option<u16>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NetworkMode {
    Server,
    Client,
    Off,
}

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        match self.mode {
            NetworkMode::Server => {
                let config = ServerConfig {
                    port: self.connect_port.unwrap_or(crate::network::DEFAULT_PORT),
                    session_token: self.session_token.clone(),
                };

                let (outgoing_tx, incoming_rx) = crate::server::spawn_server(config);
                app.insert_resource(NetworkChannels {
                    outgoing: Some(outgoing_tx),
                    client_outgoing: None,
                });
                app.insert_resource(ServerIncomingRx(incoming_rx));
                app.insert_resource(NetworkRole::Server);
            }
            NetworkMode::Client => {
                let (outgoing_tx, incoming_rx) = spawn_client();
                app.insert_resource(NetworkChannels {
                    outgoing: None,
                    client_outgoing: Some(outgoing_tx),
                });
                app.insert_resource(ClientIncomingRx(incoming_rx));
                app.insert_resource(NetworkRole::Client);

                if let Some(ref host) = self.connect_host {
                    if let Some(ref channels) = app.world().get_resource::<NetworkChannels>() {
                        if let Some(ref co) = channels.client_outgoing {
                            let _ = co.send(ClientOutgoing::Connect {
                                host: host.clone(),
                                port: self.connect_port.unwrap_or(crate::network::DEFAULT_PORT),
                                token: self.session_token.clone(),
                            });
                        }
                    }
                }
            }
            NetworkMode::Off => {
                app.insert_resource(NetworkChannels {
                    outgoing: None,
                    client_outgoing: None,
                });
                app.insert_resource(ClientIncomingRx(mpsc::unbounded_channel().1));
                app.insert_resource(NetworkRole::Off);
            }
        }

        app.init_resource::<NetworkState>()
            .add_systems(Update, process_network_incoming);
    }
}

#[derive(Resource)]
pub struct NetworkChannels {
    pub outgoing: Option<mpsc::UnboundedSender<ServerOutgoing>>,
    pub client_outgoing: Option<mpsc::UnboundedSender<ClientOutgoing>>,
}

#[derive(Resource)]
pub struct ServerIncomingRx(pub mpsc::UnboundedReceiver<ServerIncoming>);

#[derive(Resource)]
pub struct ClientIncomingRx(pub mpsc::UnboundedReceiver<ClientIncoming>);

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkRole {
    Server,
    Client,
    Off,
}

#[derive(Resource, Default)]
pub struct NetworkState {
    pub connected: bool,
    pub client_id: Option<u64>,
    pub is_gm: bool,
    pub status: String,
}

fn process_network_incoming(
    mut server_incoming: Option<ResMut<ServerIncomingRx>>,
    mut client_incoming: Option<ResMut<ClientIncomingRx>>,
    mut state: Option<ResMut<GameStateResource>>,
    mut net_state: ResMut<NetworkState>,
) {
    if let Some(ref mut rx) = server_incoming {
        while let Ok(msg) = rx.0.try_recv() {
            match msg {
                ServerIncoming::ClientConnected { client_id, is_gm } => {
                    net_state.status = format!("Client {} connected (GM: {})", client_id, is_gm);
                    info!("{}", net_state.status);
                }
                ServerIncoming::ClientDisconnected { client_id } => {
                    net_state.status = format!("Client {} disconnected", client_id);
                    info!("{}", net_state.status);
                }
                ServerIncoming::Message { client_id, message } => {
                    info!("Message from client {}: {:?}", client_id, std::mem::discriminant(&message));
                }
            }
        }
    }

    if let Some(ref mut rx) = client_incoming {
        while let Ok(msg) = rx.0.try_recv() {
            match msg {
                ClientIncoming::Connected { client_id, is_gm } => {
                    net_state.connected = true;
                    net_state.client_id = Some(client_id);
                    net_state.is_gm = is_gm;
                    net_state.status = format!("Connected as {} (id={})",
                        if is_gm { "GM" } else { "Player" }, client_id);
                    info!("{}", net_state.status);
                }
                ClientIncoming::Disconnected { reason } => {
                    net_state.connected = false;
                    net_state.client_id = None;
                    net_state.status = format!("Disconnected: {}", reason);
                    info!("{}", net_state.status);
                }
                ClientIncoming::Message(ServerMessage::StateSnapshot { history: new_history }) => {
                    if let Some(ref mut state_res) = state {
                        state_res.history = new_history;
                        info!("Applied remote state snapshot");
                    }
                }
                ClientIncoming::Message(ServerMessage::RollResult { label, roll: _ }) => {
                    let msg = format!("Roll result: {}", label);
                    net_state.status = msg.clone();
                    info!("{}", msg);
                }
                ClientIncoming::Message(ServerMessage::LogEntry { entry }) => {
                    info!("[R{}] {:?}: {}", entry.round, entry.kind, entry.message);
                }
                ClientIncoming::Message(ServerMessage::AuthSuccess { .. })
                | ClientIncoming::Message(ServerMessage::ActorOwnership { .. }) => {}
                ClientIncoming::Message(ServerMessage::AuthFailure { reason }) => {
                    net_state.connected = false;
                    net_state.status = format!("Auth failed: {}", reason);
                    warn!("Auth failed: {}", reason);
                }
                ClientIncoming::Message(ServerMessage::Error { message }) => {
                    net_state.status = format!("Server error: {}", message);
                    warn!("Server error: {}", message);
                }
                ClientIncoming::Status(s) => {
                    net_state.status = s;
                }
            }
        }
    }
}
