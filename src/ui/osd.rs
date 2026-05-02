use eframe::{NativeOptions, egui};

#[derive(PartialEq, Debug)]
pub enum OSDState {
    Hidden,
    FadingIn,
    Active,
    FadingOut,
}

pub fn options() -> NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_inner_size([220.0, 220.0])
            .with_taskbar(false),
        ..Default::default()
    }
}
