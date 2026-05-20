use bevy_egui::EguiContexts;
use crate::model::{MilSimTheme, Theme};

pub fn apply_theme(mut egui_ctx: EguiContexts) {
    let theme = MilSimTheme;
    let c = theme.colors();

    let ctx = egui_ctx.ctx_mut();
    let mut style = (*ctx.style()).clone();

    style.visuals.dark_mode = true;
    style.visuals.window_rounding = bevy_egui::egui::Rounding::same(theme.typography().panel_corner_radius);
    style.visuals.window_shadow = bevy_egui::egui::epaint::Shadow::NONE;

    let panel = bevy_egui::egui::Color32::from_rgb(
        (c.panel_surface.r * 255.0) as u8,
        (c.panel_surface.g * 255.0) as u8,
        (c.panel_surface.b * 255.0) as u8,
    );
    let border = bevy_egui::egui::Color32::from_rgb(
        (c.panel_border.r * 255.0) as u8,
        (c.panel_border.g * 255.0) as u8,
        (c.panel_border.b * 255.0) as u8,
    );
    let bg = bevy_egui::egui::Color32::from_rgb(
        (c.background.r * 255.0) as u8,
        (c.background.g * 255.0) as u8,
        (c.background.b * 255.0) as u8,
    );
    let text = bevy_egui::egui::Color32::from_rgb(
        (c.text_primary.r * 255.0) as u8,
        (c.text_primary.g * 255.0) as u8,
        (c.text_primary.b * 255.0) as u8,
    );
    let text_secondary = bevy_egui::egui::Color32::from_rgb(
        (c.text_secondary.r * 255.0) as u8,
        (c.text_secondary.g * 255.0) as u8,
        (c.text_secondary.b * 255.0) as u8,
    );
    let accent_rgb = bevy_egui::egui::Color32::from_rgb(
        (c.accent.r * 255.0) as u8,
        (c.accent.g * 255.0) as u8,
        (c.accent.b * 255.0) as u8,
    );
    let _danger = bevy_egui::egui::Color32::from_rgb(
        (c.danger.r * 255.0) as u8,
        (c.danger.g * 255.0) as u8,
        (c.danger.b * 255.0) as u8,
    );
    let _warning = bevy_egui::egui::Color32::from_rgb(
        (c.warning.r * 255.0) as u8,
        (c.warning.g * 255.0) as u8,
        (c.warning.b * 255.0) as u8,
    );
    let _success = bevy_egui::egui::Color32::from_rgb(
        (c.success.r * 255.0) as u8,
        (c.success.g * 255.0) as u8,
        (c.success.b * 255.0) as u8,
    );

    let visuals = &mut style.visuals;
    visuals.override_text_color = Some(text);
    visuals.window_fill = panel;
    visuals.panel_fill = panel;
    visuals.faint_bg_color = panel;
    visuals.extreme_bg_color = bg;
    visuals.widgets.noninteractive.bg_fill = panel;
    visuals.widgets.noninteractive.fg_stroke.color = text_secondary;
    visuals.widgets.inactive.bg_fill = border;
    visuals.widgets.inactive.fg_stroke.color = text;
    visuals.widgets.hovered.bg_fill = accent_rgb;
    visuals.widgets.hovered.fg_stroke.color = text;
    visuals.widgets.active.bg_fill = accent_rgb;
    visuals.widgets.active.fg_stroke.color = text;
    visuals.selection.bg_fill = accent_rgb;

    ctx.set_style(style);
}
