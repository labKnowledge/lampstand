#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod audio;
mod bible;
mod canvas;
mod capture;
mod engine;
mod media;
mod model;
mod permissions;
mod render;
mod services;
mod store;
mod stream;
mod text;
mod theme;
mod ui_graphics;
mod ui_library;
mod ui_outputs;
mod ui_setup;
mod ui_sources;

use eframe::egui;
use std::path::PathBuf;

pub struct LaunchOptions {
    pub screenshot: Option<PathBuf>,
    pub fresh: bool,
    pub sources: bool,
    pub projector: Option<String>,
}

fn parse_args() -> LaunchOptions {
    let mut args = std::env::args().skip(1);
    let mut opts = LaunchOptions { screenshot: None, fresh: false, sources: false, projector: None };
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => opts.screenshot = args.next().map(PathBuf::from),
            "--fresh" => opts.fresh = true,
            "--sources" => opts.sources = true,
            "--projector" => opts.projector = args.next(),
            _ => {}
        }
    }
    opts
}

fn main() -> eframe::Result {
    let opts = parse_args();
    let shooting = opts.screenshot.is_some();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Lampstand: church live & projection")
        .with_inner_size([1600.0, 940.0])
        .with_min_inner_size([1180.0, 720.0])
        .with_app_id("lampstand");
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")) {
        viewport = viewport.with_icon(icon);
    }
    if shooting {
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    #[allow(unused_mut)]
    let mut native = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    if shooting {
        native.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    eframe::run_native("Lampstand", native, Box::new(move |cc| Ok(Box::new(app::App::new(cc, opts)))))
}
