//! Asks GitHub in the background whether a newer Shelve is out: soon after start, then
//! once a day while running. Failures are quiet; the next try comes a day later.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use shelve_core::Release;

/// Not at the very start, when Windows is still busy logging in.
const FIRST_CHECK: Duration = Duration::from_secs(10);
/// Turning the switch back on.
const ON_CHECK: Duration = Duration::from_secs(1);
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

type Found = Arc<Mutex<Option<Release>>>;

pub struct UpdateCheck {
    enabled: Sender<bool>,
    found: Found,
}

impl UpdateCheck {
    pub fn start(enabled: bool, ctx: egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let found = Found::default();
        let shared = found.clone();
        let spawned = std::thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                run(rx, enabled, |release| {
                    *shared.lock().unwrap() = release;
                    ctx.request_repaint();
                })
            });
        if let Err(e) = spawned {
            println!("update check not started: {e}");
        }
        Self { enabled: tx, found }
    }

    pub fn set_enabled(&self, on: bool) {
        let _ = self.enabled.send(on);
    }

    /// A release newer than this build, once one is found.
    pub fn found(&self) -> Option<Release> {
        self.found.lock().unwrap().clone()
    }
}

fn run(rx: mpsc::Receiver<bool>, mut enabled: bool, mut report: impl FnMut(Option<Release>)) {
    let mut next = Instant::now() + FIRST_CHECK;
    loop {
        let message = if enabled {
            rx.recv_timeout(next.saturating_duration_since(Instant::now()))
        } else {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match message {
            Ok(on) => {
                if on && !enabled {
                    next = Instant::now() + ON_CHECK;
                }
                if !on {
                    report(None);
                }
                enabled = on;
            }
            Err(RecvTimeoutError::Timeout) => {
                match latest() {
                    Ok(newer) => report(newer),
                    Err(e) => println!("update check failed: {e}"),
                }
                next = Instant::now() + EVERY;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// The latest release, if it is newer than this build.
fn latest() -> anyhow::Result<Option<Release>> {
    let body = shelve_platform::https_get(
        shelve_core::RELEASES_HOST,
        shelve_core::LATEST_RELEASE_PATH,
        "Accept: application/vnd.github+json\r\n",
    )?;
    let release = shelve_core::parse_latest(&body)
        .ok_or_else(|| anyhow::anyhow!("no release in the answer"))?;
    println!("latest release: {}", release.version);
    Ok(shelve_core::is_newer(&release.version, env!("CARGO_PKG_VERSION")).then_some(release))
}
