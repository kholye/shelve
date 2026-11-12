use std::sync::{Arc, Mutex, MutexGuard};

use eframe::egui;
use shelve_core::{Config, Lang, Release};
use shelve_platform::{HotkeyError, HotkeyStatus};

use crate::theme::{contrast, Palette};
use crate::widgets::{card, lang_picker, place_label, place_picker, toggle, window_fill};

/// What the caller has to re-apply after the settings changed.
#[derive(Default)]
pub struct Changes {
    pub any: bool,
    /// Edge or auto_show: restart the mouse hook and edge sensor.
    pub hook: bool,
    /// Re-register the hotkey (restarts tray and hotkey thread).
    pub hotkey: bool,
    /// The language: the tray menu is rebuilt in it.
    pub lang: bool,
    pub autostart: bool,
    /// The update check was switched on or off.
    pub updates: bool,
}

struct State {
    open: bool,
    /// Edited copy; the app takes it together with `pending`.
    cfg: Config,
    /// Waiting for the user to press the new hotkey.
    capturing: bool,
    /// A combo that types text, waiting for "assign anyway".
    confirm: Option<String>,
    /// Why the last captured combo was not taken.
    hotkey_error: Option<String>,
    hotkey_status: Option<HotkeyStatus>,
    autostart_error: Option<String>,
    pending: Changes,
    /// Bring the window forward once it exists (also when reopened while behind others).
    want_focus: bool,
    /// Colours of the current Windows theme; refreshed on every `show`.
    palette: Palette,
    /// A newer release the update check found; refreshed on every `show`.
    update: Option<Release>,
}

/// The settings window is a deferred viewport (its own OS window, so the hotkey field can
/// take keyboard focus). Not an immediate one: eframe paints a hidden shelf from winit's
/// `new_events`, where an immediate viewport cannot create its window and egui panics.
/// Deferred viewports run on their own, so the state is shared.
#[derive(Clone)]
pub struct Settings(Arc<Mutex<State>>);

impl Settings {
    pub fn new(cfg: &Config) -> Self {
        Self(Arc::new(Mutex::new(State {
            open: false,
            cfg: cfg.clone(),
            capturing: false,
            confirm: None,
            hotkey_error: None,
            hotkey_status: None,
            autostart_error: None,
            pending: Changes::default(),
            want_focus: false,
            palette: crate::theme::palette(false, [0x0F, 0x6C, 0xBD]),
            update: None,
        })))
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // panic = "abort": a poisoned lock cannot outlive a panic.
        self.0.lock().unwrap()
    }

    pub fn open(&self, cfg: &Config, hotkey_status: Option<HotkeyStatus>) {
        let mut s = self.lock();
        if !s.open {
            s.cfg = cfg.clone();
            s.capturing = false;
            s.confirm = None;
            s.hotkey_error = None;
            s.autostart_error = None;
        }
        s.hotkey_status = hotkey_status;
        s.open = true;
        s.want_focus = true;
    }

    pub fn is_open(&self) -> bool {
        self.lock().open
    }

    /// Its OS window has been created (eframe reports it among the viewports).
    pub fn exists(&self, ctx: &egui::Context) -> bool {
        ctx.input(|i| i.raw.viewports.contains_key(&id()))
    }

    /// Stores what the window shows from outside; true if it changed.
    fn set_inputs(&self, palette: Palette, update: Option<Release>) -> bool {
        let mut s = self.lock();
        let changed = s.palette != palette || s.update != update;
        s.palette = palette;
        s.update = update;
        changed
    }

    /// Call on every pass of the shelf while open; the window closes once this stops.
    pub fn show(&self, ctx: &egui::Context, palette: Palette, update: Option<Release>) {
        if self.set_inputs(palette, update) {
            // The window only repaints on its own input: without this it stayed in the
            // old theme until the mouse moved over it.
            ctx.request_repaint_of(id());
        }
        // A window made in a later pass than the click may lose Windows' foreground
        // permission and open behind others; ask for focus once it exists.
        if self.exists(ctx) && std::mem::take(&mut self.lock().want_focus) {
            ctx.send_viewport_cmd_to(id(), egui::ViewportCommand::Focus);
        }
        let (title, height) = {
            let s = self.lock();
            // Taller while the new version card is on top, so nothing needs scrolling.
            let card = if shows_update(&s) { UPDATE_CARD_H } else { 0.0 };
            (s.cfg.lang.text().settings_title, 610.0 + card)
        };
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([420.0, height])
            .with_resizable(false)
            .with_icon(crate::window::app_icon());
        let state = self.clone();
        ctx.show_viewport_deferred(id(), builder, move |ui, _class| {
            let mut s = state.lock();
            if ui.input(|i| i.viewport().close_requested()) {
                s.open = false;
            }
            let frame = egui::Frame::new()
                .fill(window_fill(ui.visuals().dark_mode))
                .inner_margin(egui::Margin::symmetric(16, 8));
            egui::CentralPanel::default().frame(frame).show(ui, |ui| {
                // A hotkey warning or an error adds lines; the window has a fixed
                // size, so the rest must stay reachable instead of being cut off.
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| contents(ui, &mut s));
            });
            // The shelf applies changes and stops showing this window in its own pass.
            if s.pending.any || !s.open {
                ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
            }
        });
    }

    /// Edited config and what changed since the last call, if anything.
    pub fn take_changes(&self) -> Option<(Config, Changes)> {
        let mut s = self.lock();
        s.pending
            .any
            .then(|| (s.cfg.clone(), std::mem::take(&mut s.pending)))
    }

    pub fn set_feedback(
        &self,
        hotkey_status: Option<HotkeyStatus>,
        autostart_error: Option<String>,
    ) {
        let mut s = self.lock();
        s.hotkey_status = hotkey_status;
        s.autostart_error = autostart_error;
    }
}

/// Heading above a card, like a section name in Windows Settings.
fn group(ui: &mut egui::Ui, name: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(name).size(12.0).strong().weak());
}

/// One key of the hotkey, drawn like a key cap.
fn key_cap(ui: &mut egui::Ui, p: &Palette, name: &str) {
    egui::Frame::new()
        .fill(p.hover)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(9, 4))
        .show(ui, |ui| ui.label(egui::RichText::new(name).size(12.0)));
}

/// Height the new version card adds to the window, spacing included.
const UPDATE_CARD_H: f32 = 72.0;

/// The card is hidden at once when the switch goes off, before the checker hears of it.
fn shows_update(s: &State) -> bool {
    s.cfg.check_updates && s.update.is_some()
}

/// "Shelve 0.3.0 is out", with a button that opens its release page.
fn update_card(ui: &mut egui::Ui, p: &Palette, lang: Lang, release: &Release) {
    egui::Frame::new()
        .fill(p.accent_bg)
        .stroke(egui::Stroke::new(1.0, p.accent))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(egui::RichText::new(lang.update_out(&release.version)).strong());
                    let have = lang.update_have(env!("CARGO_PKG_VERSION"));
                    ui.label(egui::RichText::new(have).size(12.0).weak());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button = download_button(ui, p, lang.text().download)
                        .on_hover_text(release.url.as_str());
                    if button.clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(&release.url));
                    }
                });
            });
        });
}

/// A filled accent button with a drawn "download" arrow before its text.
fn download_button(ui: &mut egui::Ui, p: &Palette, text: &str) -> egui::Response {
    // White text where it reads on the accent; the dark theme's lighter accent takes black.
    let ink = if contrast(p.accent, egui::Color32::WHITE) >= 4.5 {
        egui::Color32::WHITE
    } else {
        egui::Color32::BLACK
    };
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), egui::FontId::proportional(13.0), ink);
    let (icon, gap, pad) = (10.0, 6.0, egui::vec2(12.0, 6.0));
    let size = egui::vec2(icon + gap + galley.size().x, galley.size().y.max(icon)) + pad * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let fill = if response.is_pointer_button_down_on() {
        p.accent.gamma_multiply(0.8)
    } else if response.hovered() {
        p.accent.gamma_multiply(0.9)
    } else {
        p.accent
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    let o = egui::pos2(rect.left() + pad.x, rect.center().y - icon / 2.0);
    let at = |x: f32, y: f32| o + egui::vec2(x, y);
    let stroke = egui::Stroke::new(1.4, ink);
    // An arrow down onto a line.
    painter.line_segment([at(5.0, 0.5), at(5.0, 7.0)], stroke);
    painter.line(vec![at(2.0, 4.0), at(5.0, 7.0), at(8.0, 4.0)], stroke);
    painter.line_segment([at(1.0, 9.5), at(9.0, 9.5)], stroke);
    let text_pos = egui::pos2(o.x + icon + gap, rect.center().y - galley.size().y / 2.0);
    painter.galley(text_pos, galley, ink);
    response
}

fn contents(ui: &mut egui::Ui, s: &mut State) {
    if s.capturing {
        capture(ui, s);
    }
    if shows_update(s) {
        if let Some(release) = &s.update {
            ui.add_space(8.0);
            update_card(ui, &s.palette, s.cfg.lang, release);
        }
    }
    let State {
        open: _,
        cfg,
        capturing,
        confirm,
        hotkey_error,
        hotkey_status,
        autostart_error,
        pending: changes,
        want_focus: _,
        palette,
        update: _,
    } = s;
    let p = &*palette;
    let lang = cfg.lang;
    let t = lang.text();
    ui.spacing_mut().item_spacing.y = 8.0;

    group(ui, t.place);
    card(ui, p, |ui| {
        ui.horizontal(|ui| {
            let (edge_changed, align_changed) =
                place_picker(ui, lang, &mut cfg.edge, &mut cfg.align);
            changes.hook |= edge_changed;
            changes.any |= align_changed;
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(place_label(lang, cfg.edge, cfg.align));
                ui.label(egui::RichText::new(t.place_hint).size(12.0).weak());
            });
        });
    });

    group(ui, t.hotkey);
    card(ui, p, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(t.hotkey_what);
                ui.add_space(2.0);
                if *capturing {
                    ui.label(egui::RichText::new(t.hotkey_press).weak());
                } else {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for name in cfg.hotkey.split('+') {
                            key_cap(ui, p, name);
                        }
                    });
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button = egui::Button::new(t.change).min_size(egui::vec2(0.0, 28.0));
                if ui.add(button).on_hover_text(t.change_hint).clicked() {
                    *capturing = true;
                    *confirm = None;
                    *hotkey_error = None;
                }
            });
        });
    });
    let error = ui.visuals().error_fg_color;
    if let Some(e) = hotkey_error.as_ref().filter(|_| !*capturing) {
        ui.colored_label(error, e.as_str());
    } else if let (false, Some(HotkeyStatus::Failed(e))) = (*capturing, &*hotkey_status) {
        // None right after a change: the new status comes on the shelf's next pass.
        let why = lang.hotkey_status_error(&cfg.hotkey, *e);
        ui.colored_label(error, lang.hotkey_failed(&why));
    }
    if let Some(combo) = confirm.clone() {
        let warn = ui.visuals().warn_fg_color;
        ui.colored_label(warn, lang.hotkey_breaks_typing(&combo));
        ui.horizontal(|ui| {
            if ui.button(t.assign_anyway).clicked() {
                *confirm = None;
                assign(combo, cfg, hotkey_error, hotkey_status, changes);
            }
            if ui.button(t.cancel).clicked() {
                *confirm = None;
            }
        });
    }

    group(ui, t.behavior);
    card(ui, p, |ui| {
        changes.hook |= toggle(ui, &mut cfg.auto_show, t.auto_show).changed();
    });
    card(ui, p, |ui| {
        changes.autostart |= toggle(ui, &mut cfg.autostart, t.autostart).changed();
    });
    if let Some(e) = autostart_error {
        ui.colored_label(error, e.as_str());
    }
    card(ui, p, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        changes.updates |= toggle(ui, &mut cfg.check_updates, t.check_updates).changed();
        ui.label(egui::RichText::new(t.check_updates_hint).size(12.0).weak());
    });

    group(ui, t.language);
    card(ui, p, |ui| {
        ui.horizontal(|ui| {
            ui.label(t.language);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if lang_picker(ui, &mut cfg.lang) {
                    changes.lang = true;
                    // It was written in the language it was shown in.
                    *hotkey_error = None;
                }
            });
        });
    });

    ui.add_space(2.0);
    let version = format!("{} {}", t.version, env!("CARGO_PKG_VERSION"));
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(version).size(12.0).weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            project_link(ui, t.project_page);
        });
    });
    changes.any |=
        changes.hook || changes.hotkey || changes.autostart || changes.lang || changes.updates;
}

const PROJECT_URL: &str = "https://github.com/kholye/shelve";

/// The project page link: text and a drawn "opens outside" arrow, one click target.
fn project_link(ui: &mut egui::Ui, text: &str) {
    let color = ui.visuals().hyperlink_color;
    let font = egui::FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let (icon, gap) = (10.0, 4.0);
    let size = egui::vec2(galley.size().x + gap + icon, galley.size().y);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(PROJECT_URL);
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(PROJECT_URL));
    }
    let painter = ui.painter();
    let text_pos = rect.left_top();
    if response.hovered() {
        let y = text_pos.y + galley.size().y - 1.0;
        let x = text_pos.x..=text_pos.x + galley.size().x;
        painter.hline(x, y, egui::Stroke::new(1.0, color));
    }
    painter.galley(text_pos, galley, color);
    let o = egui::pos2(rect.right() - icon, rect.center().y - icon / 2.0);
    let at = |x: f32, y: f32| o + egui::vec2(x, y);
    let stroke = egui::Stroke::new(1.2, color);
    // A box open at the top right, and an arrow leaving through the gap.
    let corners = [(4.0, 1.0), (1.0, 1.0), (1.0, 9.0), (9.0, 9.0), (9.0, 6.0)];
    painter.line(corners.map(|(x, y)| at(x, y)).to_vec(), stroke);
    painter.line(vec![at(6.0, 1.0), at(9.0, 1.0), at(9.0, 4.0)], stroke);
    painter.line_segment([at(9.0, 1.0), at(4.5, 5.5)], stroke);
}

fn id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("settings")
}

/// Takes the first key press as the new hotkey; Esc cancels.
fn capture(ui: &egui::Ui, s: &mut State) {
    let Some((key, mods)) = ui.input(|i| pressed_combo(&i.events)) else {
        return;
    };
    s.capturing = false;
    if key == egui::Key::Escape {
        return;
    }
    let combo = combo_from(key, mods);
    if breaks_typing(key, mods) {
        s.confirm = Some(combo);
    } else {
        assign(
            combo,
            &mut s.cfg,
            &mut s.hotkey_error,
            &mut s.hotkey_status,
            &mut s.pending,
        );
    }
}

/// The first key press in `events` that can be a hotkey's main key. egui also reports
/// Ctrl/Shift/Alt/Win themselves as keys; those only count as modifiers.
fn pressed_combo(events: &[egui::Event]) -> Option<(egui::Key, egui::Modifiers)> {
    use egui::Key::*;
    events.iter().find_map(|e| match e {
        egui::Event::Key {
            key:
                ShiftLeft | ShiftRight | ControlLeft | ControlRight | AltLeft | AltRight | SuperLeft
                | SuperRight,
            ..
        } => None,
        egui::Event::Key {
            key,
            pressed: true,
            repeat: false,
            modifiers,
            ..
        } => Some((*key, *modifiers)),
        _ => None,
    })
}

/// Saves `combo` as the hotkey, or keeps the old one and says why not.
fn assign(
    combo: String,
    cfg: &mut Config,
    hotkey_error: &mut Option<String>,
    hotkey_status: &mut Option<HotkeyStatus>,
    changes: &mut Changes,
) {
    if combo == cfg.hotkey {
        return;
    }
    match shelve_platform::try_hotkey(&combo) {
        Ok(()) => {
            cfg.hotkey = combo;
            changes.hotkey = true;
            changes.any = true;
            *hotkey_status = None; // the old "works" belongs to the old hotkey
        }
        Err(HotkeyError::Unsupported) => {
            *hotkey_error = Some(cfg.lang.hotkey_unsupported(&combo));
        }
        Err(HotkeyError::Taken) => {
            *hotkey_error = Some(cfg.lang.hotkey_taken(&combo, &cfg.hotkey));
        }
    }
}

/// Hotkey string in the format global-hotkey parses, e.g. "Ctrl+Alt+D".
pub fn combo_from(key: egui::Key, mods: egui::Modifiers) -> String {
    let mut parts = Vec::new();
    for (on, name) in [
        (mods.ctrl, "Ctrl"),
        (mods.alt, "Alt"),
        (mods.shift, "Shift"),
    ] {
        if on {
            parts.push(name);
        }
    }
    // global-hotkey knows these by their symbol, egui names them by word.
    let name = match key {
        egui::Key::Equals
        | egui::Key::OpenBracket
        | egui::Key::CloseBracket
        | egui::Key::Backtick => key.symbol_or_name(),
        _ => key.name(),
    };
    parts.push(name);
    parts.join("+")
}

/// A key that types text when pressed alone (or with only Shift): as a global hotkey it
/// would stop working in every other program.
pub fn breaks_typing(key: egui::Key, mods: egui::Modifiers) -> bool {
    let types_text = key.symbol_or_name().chars().count() == 1
        || matches!(
            key,
            egui::Key::Space | egui::Key::Enter | egui::Key::Backspace | egui::Key::Tab
        );
    types_text && !mods.ctrl && !mods.alt
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Key, Modifiers};

    const CTRL_ALT: Modifiers = Modifiers {
        alt: true,
        ctrl: true,
        shift: false,
        mac_cmd: false,
        command: true,
    };

    #[test]
    fn a_new_palette_repaints_the_open_settings_window() {
        // Bug: the window stayed in the old theme until the mouse moved over it.
        // A headless egui keeps a never-painted viewport's repaint request pending, so
        // only the decision is tested: `show` repaints the window when this says true.
        let settings = Settings::new(&Config::default());
        let palette = |dark| crate::theme::palette(dark, [0x0F, 0x6C, 0xBD]);
        assert!(!settings.set_inputs(palette(false), None), "same palette");
        assert!(settings.set_inputs(palette(true), None));
        assert!(!settings.set_inputs(palette(true), None));
    }

    #[test]
    fn a_found_release_repaints_the_open_settings_window() {
        let settings = Settings::new(&Config::default());
        let palette = crate::theme::palette(false, [0x0F, 0x6C, 0xBD]);
        let release = Release {
            version: "9.0.0".into(),
            url: "https://github.com/kholye/shelve/releases/tag/v9.0.0".into(),
        };
        assert!(settings.set_inputs(palette, Some(release.clone())));
        assert!(shows_update(&settings.lock()));
        assert!(!settings.set_inputs(palette, Some(release)));
        settings.lock().cfg.check_updates = false;
        assert!(!shows_update(&settings.lock()), "switched off: no card");
    }

    #[test]
    fn combo_lists_modifiers_then_key() {
        assert_eq!(combo_from(Key::D, CTRL_ALT), "Ctrl+Alt+D");
        assert_eq!(combo_from(Key::F2, Modifiers::SHIFT), "Shift+F2");
        assert_eq!(combo_from(Key::F9, Modifiers::NONE), "F9");
    }

    #[test]
    fn every_supported_key_parses_as_global_hotkey() {
        // A few egui keys have no global-hotkey code; try_hotkey rejects those at capture time.
        let unsupported: Vec<_> = Key::ALL
            .iter()
            .filter(|k| {
                combo_from(**k, Modifiers::CTRL)
                    .parse::<global_hotkey::hotkey::HotKey>()
                    .is_err()
            })
            .collect();
        println!("unsupported: {unsupported:?}");
        assert!(
            unsupported.len() < Key::ALL.len() / 4,
            "too many: {unsupported:?}"
        );
    }

    fn press(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn modifier_presses_are_not_the_hotkey() {
        // Bug: holding Ctrl, then Alt, then F recorded "Ctrl+ControlLeft".
        let events = [
            press(Key::ControlLeft, Modifiers::CTRL),
            press(Key::AltLeft, CTRL_ALT),
            press(Key::F, CTRL_ALT),
        ];
        assert_eq!(pressed_combo(&events), Some((Key::F, CTRL_ALT)));
        assert_eq!(pressed_combo(&events[..2]), None);
    }

    fn assign_to(combo: &str) -> (Config, Option<String>, Option<HotkeyStatus>, Changes) {
        let mut cfg = Config::default();
        let (mut error, mut status) = (None, Some(HotkeyStatus::Registered));
        let mut changes = Changes::default();
        assign(
            combo.into(),
            &mut cfg,
            &mut error,
            &mut status,
            &mut changes,
        );
        (cfg, error, status, changes)
    }

    #[test]
    fn assign_clears_the_old_status() {
        // Bug: "works" was shown for the new hotkey before it was registered.
        let (cfg, error, status, changes) = assign_to("Ctrl+Alt+Shift+F22");
        assert_eq!(cfg.hotkey, "Ctrl+Alt+Shift+F22");
        assert!(changes.hotkey && changes.any);
        assert_eq!((error, status), (None, None));
    }

    #[test]
    fn taken_hotkey_keeps_the_old_one() {
        let taken: global_hotkey::hotkey::HotKey = "Ctrl+Alt+Shift+F23".parse().unwrap();
        let holder = global_hotkey::GlobalHotKeyManager::new().unwrap();
        holder.register(taken).unwrap();
        let (cfg, error, _, changes) = assign_to("Ctrl+Alt+Shift+F23");
        assert_eq!(cfg.hotkey, Config::default().hotkey);
        assert!(!changes.any);
        assert!(error.unwrap().contains("taken"));
    }

    #[test]
    fn typing_keys_alone_are_flagged() {
        assert!(breaks_typing(Key::D, Modifiers::NONE));
        assert!(breaks_typing(Key::D, Modifiers::SHIFT));
        assert!(breaks_typing(Key::Space, Modifiers::NONE));
        assert!(breaks_typing(Key::Enter, Modifiers::NONE));
        assert!(breaks_typing(Key::Equals, Modifiers::NONE));
        assert!(!breaks_typing(Key::D, CTRL_ALT));
        assert!(!breaks_typing(Key::F9, Modifiers::NONE));
    }
}
