/// Device tab UI component.
///
/// Renders the device settings panel, including the default function key behavior switcher.
use eframe::egui;

use super::Settings;
use crate::razer::device_handle::device;
use crate::ui::app::app;
use crate::ui::app_events::OsdEvent;

pub fn show(ui: &mut eframe::egui::Ui, ctx: &egui::Context, settings: &mut Settings) {
    default_func_key_switcher(ui, ctx, settings);
}

pub(super) fn default_func_key_switcher(
    ui: &mut eframe::egui::Ui,
    ctx: &egui::Context,
    settings: &mut Settings,
) {
    let mut changed = false;
    ui.label("Default Function key behaviour:");
    ui.horizontal(|ui| {
        if let Some(config) = settings.app_config.as_mut() {
            changed |= ui
                .selectable_value(&mut config.default_multimedia_keys, false, "Function")
                .changed();
            changed |= ui
                .selectable_value(&mut config.default_multimedia_keys, true, "Multimedia")
                .changed();
        }
        if changed {
            let mode = device()
                .toggle_default_multimedia_keys()
                .unwrap_or_default();
            app(OsdEvent::ToggleDefaultMultimediaKeys(mode).into());
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    });
}
