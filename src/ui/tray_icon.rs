use crate::razer::device_handle::device;

use resvg::{tiny_skia, usvg};
use std::sync::OnceLock;
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};
use winit::event_loop::{EventLoop, EventLoopBuilder, EventLoopProxy};

pub static GUI_PROXY: OnceLock<EventLoopProxy<u8>> = OnceLock::new();

pub struct TrayIcon {
    tray_icon: tray_icon::TrayIcon,
    event_loop: Option<EventLoop<u8>>,
}

impl TrayIcon {
    pub fn new() -> Self {
        let icon = load_tray_icon("#95A5A6");

        let tray_menu = Menu::new();
        let quit_item = MenuItem::new("Quit", true, None);
        let quit_id = quit_item.id().0.clone();
        let restart_item = MenuItem::new("Restart", true, None);
        let restart_id = restart_item.id().0.clone();
        tray_menu.append(&quit_item).unwrap();
        tray_menu.append(&restart_item).unwrap();

        let tray_icon = TrayIconBuilder::new()
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

        MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id {
            id if id == quit_id => {
                let _ = proxy.send_event(255);
            }
            id if id == restart_id => {
                let _ = proxy.send_event(254);
            }
            _ => {}
        }));
        Self {
            tray_icon,
            event_loop: Some(event_loop),
        }
    }

    pub fn run(&mut self) {
        self.event_loop
            .take()
            .expect("Fatal internal error: event loop")
            .run(move |event, event_loop| {
                event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
                match event {
                    winit::event::Event::UserEvent(data) => {
                        match data {
                            255 => {
                                println!("Menu: Quit clicked");
                                device().persist_config();
                                event_loop.exit();
                            }
                            254 => {
                                println!("Restarting...");
                                device().persist_config();
                                restart_app()
                            }
                            mode => {
                                println!("Update: Switching to mode {}", mode);
                                let hex = match mode {
                                    5 => "#00C853", // Silent
                                    6 => "#00E5FF", // Quiet? (This is not exposed in Syanpse)
                                    0 => "#FFD600", // Balanced
                                    2 => "#FF5D00", // Performance
                                    1 => "#D50000", // Turbo
                                    4 => "#A200FF", // Custom
                                    _ => "#95A5A6", // Concrete Grey (Idle / Unknown)
                                };

                                let new_icon = load_tray_icon(hex);
                                self.tray_icon
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
}

fn load_tray_icon(hex_color: &str) -> Icon {
    let width = 64;
    let height = 64;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();

    let coloured_svg = include_str!("../../assets/icon.svg")
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

fn restart_app() {
    let current_exe = std::env::current_exe().expect("Failed to get current exe path");
    std::process::Command::new(current_exe)
        .spawn()
        .expect("Failed to restart");
    std::process::exit(0);
}
