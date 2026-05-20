use crate::model::{AppSettings, MilSimTheme, Srgba, Theme, ThemeColors, ThemeTypography};
use bevy::prelude::*;
use std::path::Path;

const SETTINGS_FILENAME: &str = "fantactical_settings.json";

#[derive(Resource, Debug, Clone, Default)]
pub struct SettingsResource {
    pub settings: AppSettings,
}

pub fn load_settings() -> AppSettings {
    let path = Path::new(SETTINGS_FILENAME);
    if path.exists() {
        match std::fs::read_to_string(path) {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(settings) => {
                    info!("Loaded settings from {}", SETTINGS_FILENAME);
                    return settings;
                }
                Err(e) => warn!("Failed to parse settings file: {e}, using defaults"),
            },
            Err(e) => warn!("Failed to read settings file: {e}, using defaults"),
        }
    }
    info!("No settings file found, using defaults (will be saved on exit)");
    AppSettings::default()
}

pub fn save_settings(settings: &AppSettings) {
    match serde_json::to_string_pretty(settings) {
        Ok(json) => {
            if let Err(e) = std::fs::write(SETTINGS_FILENAME, &json) {
                error!("Failed to write settings file: {e}");
            }
        }
        Err(e) => error!("Failed to serialize settings: {e}"),
    }
}

pub fn register_themes() -> Vec<Box<dyn Theme + Send + Sync>> {
    vec![Box::new(MilSimTheme)]
}

pub fn get_theme_colors(theme_id: &str) -> ThemeColors {
    let themes = register_themes();
    themes
        .iter()
        .find(|t| t.id() == theme_id)
        .map(|t| t.colors())
        .unwrap_or_else(|| MilSimTheme.colors())
}

pub fn get_theme_typography(theme_id: &str) -> ThemeTypography {
    let themes = register_themes();
    themes
        .iter()
        .find(|t| t.id() == theme_id)
        .map(|t| t.typography())
        .unwrap_or_else(|| MilSimTheme.typography())
}

pub fn srgb_to_bevy_color(c: Srgba) -> Color {
    Color::srgba(c.r, c.g, c.b, c.a)
}
