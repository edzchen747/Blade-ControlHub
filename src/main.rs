mod razer;
mod win;

use resvg::{tiny_skia, usvg};
use std::sync::OnceLock;
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};
use winit::event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy};

pub static GUI_PROXY: OnceLock<EventLoopProxy<u8>> = OnceLock::new();

fn main() {
    let icon = load_tray_icon("#95A5A6");

    let tray_menu = Menu::new();
    let quit_item = MenuItem::new("Quit", true, None);
    let quit_id = quit_item.id().0.clone();
    tray_menu.append(&quit_item).unwrap();

    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Blade ControlHub")
        .with_icon(icon)
        .build()
        .unwrap();

    let event_loop: EventLoop<u8> = EventLoopBuilder::with_user_event()
        .build()
        .expect("Failed to build event loop");
    let proxy = event_loop.create_proxy();
    GUI_PROXY
        .set(proxy.clone())
        .expect("Fatal internal error: set gui proxy");

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == quit_id {
            let _ = proxy.send_event(255); // Use 255 for "Quit"
        }
    }));

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.spawn(async {
        println!("Razer service started...");
        razer_service()
    });

    event_loop
        .run(move |event, event_loop| {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
            match event {
                winit::event::Event::UserEvent(data) => {
                    match data {
                        255 => {
                            println!("Menu: Quit clicked");
                            event_loop.exit();
                        }
                        mode => {
                            println!("Update: Switching to mode {}", mode);
                            let hex = match mode {
                                5 => "#00C853", // Emerald Green (Efficiency / Eco)
                                6 => "#00E5FF", // Sun Flower Yellow (NEW: Between Green and Orange)
                                0 => "#FFD600", // Peter River Blue (Standard / Balanced)
                                2 => "#FF5D00", // Carrot Orange (Boost / High)
                                1 => "#D50000", // Alizarin Red (Extreme / Turbo)
                                4 => "#A200FF", // Amethyst Purple (Max / Overdrive)
                                _ => "#95A5A6", // Concrete Grey (Idle / Unknown)
                            };

                            let new_icon = load_tray_icon(hex);
                            _tray_icon
                                .set_icon(Some(new_icon))
                                .expect("Failed to update icon");
                        }
                    }
                }
                // for future if window is created
                winit::event::Event::WindowEvent { event, .. } => match event {
                    winit::event::WindowEvent::CloseRequested => event_loop.exit(),
                    _ => (),
                },
                _ => (),
            }
        })
        .unwrap();
    std::process::exit(0);
}

fn load_tray_icon(hex_color: &str) -> Icon {
    let width = 64;
    let height = 64;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();

    let coloured_svg = include_str!("../assets/icon.svg")
        .replace("#FFFFFF", hex_color)
        .replace("#ffffff", &hex_color.to_lowercase());

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(&coloured_svg, &opt).expect("Failed to parse SVG");

    let svg_size = tree.size();

    // Sscale it up by an extra 20-30%
    let base_scale = (width as f32 / svg_size.width()).min(height as f32 / svg_size.height());
    let final_scale = base_scale * 1.2;

    // Recalculate centering for the overscaled icon
    let tx = (width as f32 - (svg_size.width() * final_scale)) / 2.0;
    let ty = (height as f32 - (svg_size.height() * final_scale)) / 2.0;

    let transform =
        tiny_skia::Transform::from_scale(final_scale, final_scale).post_translate(tx, ty);

    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let rgba = pixmap.take();
    Icon::from_rgba(rgba, width, height).expect("Failed to create tray icon")
}

fn razer_service() -> Result<(), anyhow::Error> {
    let device_pid = razer::device_handle::device().get_pid();
    let _ = razer::device_handle::device().get_perf_mode();

    win::key_hook::init_keyboard_hooks(device_pid)?;

    razer::device_handle::device().initialize_device();

    win::standby::spawn_listener_thread()
}
