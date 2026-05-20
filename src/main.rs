use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use fantactical::logging::LoggingPlugin;
use fantactical::settings::SettingsResource;
use fantactical::systems::persistence::PersistencePlugin;
use fantactical::systems::phase_machine::PhaseMachinePlugin;
use fantactical::systems::network::{NetworkPlugin, NetworkMode};
use fantactical::ui::battlemap::BattlemapPlugin;
use fantactical::ui::panels::PanelsPlugin;
use fantactical::ui::roll_modal::RollModalPlugin;

fn main() {
    let settings = fantactical::settings::load_settings();

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_plugins(LoggingPlugin)
        .insert_resource(SettingsResource { settings })
        .add_plugins(NetworkPlugin {
            mode: NetworkMode::Off,
            session_token: "fantactical".to_string(),
            connect_host: None,
            connect_port: None,
        })
        .add_plugins(BattlemapPlugin)
        .add_plugins(PanelsPlugin)
        .add_plugins(PhaseMachinePlugin)
        .add_plugins(RollModalPlugin)
        .add_plugins(PersistencePlugin)
        .run();
}
