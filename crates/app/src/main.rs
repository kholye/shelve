// Without it a console window opens and takes the focus.
#![windows_subsystem = "windows"]

mod app;
mod auto_show;
mod labels;
mod layout;
mod presence;
mod rows;
mod settings;
mod shelf_view;
mod theme;
mod widgets;
mod window;

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Instant;

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use shelve_core::{default_config_path, Config, ScreenRect, Shelf};

use app::ShelfApp;
use settings::Settings;

fn main() -> eframe::Result {
    let _instance = shelve_platform::acquire_single_instance();
    let cfg = Config::load(&default_config_path());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Shelve")
            .with_icon(window::app_icon())
            .with_inner_size(layout::window_size(layout::EMPTY_H))
            .with_visible(false)
            // eframe flashes the window after the first frame; keep that off-screen.
            .with_position([-32000.0, -32000.0])
            .with_decorations(false)
            .with_resizable(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(false)
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Shelve",
        options,
        Box::new(move |cc| {
            let RawWindowHandle::Win32(h) = cc.window_handle()?.as_raw() else {
                return Err("not a Win32 window".into());
            };
            let hwnd = h.hwnd.get();
            theme::install_font(&cc.egui_ctx);
            let look = shelve_platform::system_look();
            let palette = theme::apply(&cc.egui_ctx, look);
            shelve_platform::prepare_shelf_window(hwnd);

            // The platform channel is read off the UI thread; a hidden, sleeping
            // window only wakes up through request_repaint.
            let (platform_tx, platform_rx) = mpsc::channel();
            let (app_tx, events) = mpsc::channel();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                for ev in platform_rx {
                    if app_tx.send(ev).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
            // One long-lived thread asks the disk; its answers wake the UI like events do.
            let (presence_queue, checker) = presence::queue();
            let (answer_tx, presence_answers) = mpsc::channel();
            let ctx = cc.egui_ctx.clone();
            std::thread::Builder::new()
                .name("presence".into())
                .spawn(move || {
                    checker.run(|path, a| {
                        let _ = answer_tx.send((path, a));
                        ctx.request_repaint();
                    })
                })?;
            let shared = layout::SharedRect::default();
            let mouse_hook = app::start_auto_show(&cfg, platform_tx.clone(), shared.clone());
            let platform = shelve_platform::start(&cfg, platform_tx.clone())?;
            println!("hotkey {}: {:?}", cfg.hotkey, platform.hotkey_status());
            if cfg.autostart {
                // Points the Run entry at this exe, in case it was moved.
                let _ = shelve_platform::set_autostart(true);
            }
            Ok(Box::new(ShelfApp {
                shelf: Shelf::new(),
                settings: Settings::new(&cfg),
                cfg,
                events,
                platform_tx,
                hwnd,
                visible: false,
                target: false,
                slide_start: Instant::now(),
                slide_from: 1.0,
                release_pointer: false,
                platform: Some(platform),
                mouse_hook,
                icons: HashMap::new(),
                presence: HashMap::new(),
                presence_queue,
                presence_answers,
                look,
                palette,
                screen: ScreenRect::default(),
                scale: 1.0,
                rows: Default::default(),
                animate: true,
                height: rows::Glide::at_rest(layout::EMPTY_H, Instant::now()),
                placed_h: None,
                win: ScreenRect::default(),
                shared,
                file_over: false,
            }))
        }),
    )
}
