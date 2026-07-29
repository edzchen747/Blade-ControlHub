// #![windows_subsystem = "windows"]

// use blade_controlhub::ui::settings::Settings;
// use blade_controlhub::ui::settings::store::SettingsStore;
// use blade_controlhub::ui::theme::{
//     SETTINGS_PADDING_RATIO, SETTINGS_WINDOW_SIZE, SETTINGS_WINDOW_TITLE,
// };
// use eframe::egui;

// struct SettingsApp {
//     settings: SettingsStore,
// }

// impl eframe::App for SettingsApp {
//     fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
//         if let Ok(mut settings_lock) = self.settings.inner.lock() {
//             settings_lock.ui(ctx);
//         }
//     }
// }

// fn main() -> eframe::Result<()> {
//     let icon_data = Settings::load_settings_icon();
//     let window_size = SETTINGS_WINDOW_SIZE;

//     let mut native_options = eframe::NativeOptions::default();

//     // Configure the base viewport parameters first
//     let mut viewport = egui::ViewportBuilder::default()
//         .with_title(SETTINGS_WINDOW_TITLE)
//         .with_icon(icon_data)
//         .with_inner_size(window_size)
//         .with_min_inner_size(window_size)
//         .with_max_inner_size(window_size)
//         .with_resizable(false)
//         .with_maximize_button(false);

//     // Dynamically query the primary monitor sizing using the available systems
//     // to calculate the exact bottom right corner coordinates before window realization
//     if let Some(monitor) = pre_resolve_monitor_dimensions() {
//         let screen_width = monitor.width as f32;
//         let screen_height = monitor.height as f32;

//         let padding = screen_height * SETTINGS_PADDING_RATIO;

//         // This is your exact original positioning equation
//         let spawn_pos = egui::pos2(
//             screen_width - window_size.x - padding * 0.1,
//             screen_height - window_size.y - padding,
//         );
//         viewport = viewport.with_position(spawn_pos);
//     }

//     native_options.viewport = viewport;

//     eframe::run_native(
//         "blade_settings_window",
//         native_options,
//         Box::new(|_cc| {
//             Box::new(SettingsApp {
//                 settings: SettingsStore::new(),
//             })
//         }),
//     )
// }

// struct SimpleMonitor {
//     width: u32,
//     height: u32,
// }

// /// Helper using basic Win32 API metrics to get logical display dimensions
// /// accounting for active scale factors.
// fn pre_resolve_monitor_dimensions() -> Option<SimpleMonitor> {
//     #[cfg(target_os = "windows")]
//     unsafe {
//         use windows_sys::Win32::UI::WindowsAndMessaging::{
//             GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
//         };
//         // Get primary monitor logical resolution width and height
//         let width = GetSystemMetrics(SM_CXSCREEN);
//         let height = GetSystemMetrics(SM_CYSCREEN);
//         if width > 0 && height > 0 {
//             return Some(SimpleMonitor {
//                 width: width as u32,
//                 height: height as u32,
//             });
//         }
//     }
//     None
// }
