//! Manual check: tray menu, hotkey and single instance. Prints every PlatformEvent.
//! Run: cargo run -p shelve-platform --example smoke [-- <hotkey>]

fn main() -> anyhow::Result<()> {
    let _instance = shelve_platform::acquire_single_instance();
    let mut cfg = shelve_core::Config::default();
    if let Some(hk) = std::env::args().nth(1) {
        cfg.hotkey = hk;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = shelve_platform::start(&cfg, tx)?;
    println!("hotkey {}: {:?}", cfg.hotkey, handle.hotkey_status());
    for ev in rx {
        println!("{ev:?}");
        if ev == shelve_platform::PlatformEvent::TrayQuit {
            break;
        }
    }
    Ok(())
}
