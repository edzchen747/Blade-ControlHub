/// Appearance tab UI component.
///
/// Placeholder for theme/appearance customization settings.
use eframe::egui;

pub fn show(ui: &mut eframe::egui::Ui, _ctx: &egui::Context) {
    ui.label("Text Color:");
    ui.color_edit_button_rgba_unmultiplied(&mut [1.0, 1.0, 1.0, 1.0]);
    // Colour picker WIP
}
