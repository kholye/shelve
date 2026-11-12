use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use eframe::egui;
use shelve_core::{
    default_config_path, Command, Config, Edge, ItemKind, ScreenRect, Shelf, ShelfItem,
};

use crate::auto_show::AutoShowInput;
use crate::presence::{self, is_missing, Presence};
use crate::shelf_view::{shelf_view, Action};
use shelve_platform::{
    DragButton, DragOutcome, HotkeyStatus, MouseHook, PlatformEvent, PlatformHandle, SystemLook,
    TabColors, TabSpec,
};

use crate::layout::{self, SharedRect};
use crate::rows::{ease_out, Glide, Rows, ANIM_S};
use crate::settings::{Changes, Settings};
use crate::theme::{self, Palette};
use crate::update::UpdateCheck;
use crate::window::{shelf_rect, tab_rect};

/// Slide-in duration, seconds.
const SLIDE_S: f32 = ANIM_S;

pub struct ShelfApp {
    pub shelf: Shelf,
    pub cfg: Config,
    pub events: Receiver<PlatformEvent>,
    /// Kept so tray/hotkey and the hook can be restarted with fresh settings.
    pub platform_tx: Sender<PlatformEvent>,
    pub hwnd: isize,
    /// The window is on screen (also while sliding out).
    pub visible: bool,
    /// Where the slide goes: true = in, false = out (then the window is hidden).
    pub target: bool,
    pub slide_start: Instant,
    /// Offset at `slide_start`: 0 = fully in, 1 = fully behind the edge.
    pub slide_from: f32,
    /// OLE ate the mouse-up during DoDragDrop; egui still thinks this button is held.
    pub release_pointer: Option<egui::PointerButton>,
    /// Dropping it stops the platform thread and removes the tray icon.
    pub platform: Option<PlatformHandle>,
    pub mouse_hook: Option<MouseHook>,
    /// The thread that alone decides whether the shelf is shown (`auto_show`).
    pub auto_show: Sender<AutoShowInput>,
    pub settings: Settings,
    pub updates: UpdateCheck,
    /// None: the shell had no icon, the drawn glyph is used.
    /// Note: never pruned; a few KB per path ever dropped in one session.
    pub icons: HashMap<PathBuf, Option<egui::TextureHandle>>,
    /// Which files are gone, as last answered by the presence thread. Drawing only reads it.
    pub presence: Presence,
    /// Asks the presence thread; its answers come back on `presence_answers`.
    pub presence_queue: presence::Queue,
    pub presence_answers: Receiver<presence::Answer>,
    /// What Windows looked like when `palette` was made.
    pub look: SystemLook,
    pub palette: Palette,
    /// Work area and DPI scale of the monitor the shelf was last shown on.
    pub screen: ScreenRect,
    pub scale: f32,
    /// The items as drawn: with rows still growing in or collapsing.
    pub rows: Rows,
    /// "Animation effects" are on in Windows; read every time the shelf is shown.
    pub animate: bool,
    /// The panel height; glides when items come or go.
    pub height: Glide,
    /// Panel height the window was last sized for; None while hidden.
    pub placed_h: Option<f32>,
    /// Where the window was last put, physical pixels.
    pub win: ScreenRect,
    /// Tells the auto-hide thread where the panel is.
    pub shared: SharedRect,
    /// A file is being carried over the shelf right now.
    pub file_over: bool,
    /// What the tab was last told; None while it is hidden.
    pub tab: Option<TabSpec>,
}

/// Starts the mouse hook and edge sensor; None if "show while dragging" is off. Its events
/// go to the `auto_show` thread, which outlives the hook: a restart keeps the shelf state.
pub fn start_mouse_hook(cfg: &Config, auto_show: Sender<AutoShowInput>) -> Option<MouseHook> {
    if !cfg.auto_show {
        return None;
    }
    let on_event = Box::new(move |e| {
        let _ = auto_show.send(AutoShowInput::Hook(e));
    });
    // Without the hook only the hotkey and tray show the shelf.
    shelve_platform::start_mouse_hook(cfg.edge, cfg.edge_zone_px, on_event)
        .inspect_err(|e| println!("mouse hook failed: {e}"))
        .ok()
}

impl ShelfApp {
    fn slide_t(&self) -> f32 {
        if !self.animate {
            return 1.0;
        }
        (self.slide_start.elapsed().as_secs_f32() / SLIDE_S).min(1.0)
    }

    /// Current offset, 0 = in, 1 = out; eases out so both directions start fast.
    fn offset(&self) -> f32 {
        let eased = ease_out(self.slide_t());
        let to = if self.target { 0.0 } else { 1.0 };
        self.slide_from + (to - self.slide_from) * eased
    }

    /// Slides in or out; a change mid-slide turns around from where it is.
    fn set_visible(&mut self, visible: bool) {
        if !visible && !self.visible {
            return;
        }
        // Already fully out and asked to be shown again (a setting changed): there is
        // nothing to slide. Restarting the slide made the shelf repaint for its whole
        // duration, and the settings window got no frames meanwhile: its switches
        // jumped instead of sliding.
        let in_place = visible && self.visible && self.target && self.slide_t() >= 1.0;
        if !in_place {
            self.slide_from = if self.visible { self.offset() } else { 1.0 };
            self.slide_start = Instant::now();
        }
        self.target = visible;
        if visible {
            (self.screen, self.scale) = shelve_platform::monitor_at_cursor();
            self.animate = shelve_platform::animations_enabled();
            let now = Instant::now();
            if !self.visible {
                for item in self.shelf.items() {
                    self.presence_queue.check(item.path.clone());
                }
                // Whatever changed while hidden is simply there: nothing plays on show.
                self.rows.sync(self.shelf.items(), now, false);
                self.height = Glide::at_rest(self.target_h(), now);
            }
            self.place(now);
            self.visible = true;
        }
        // Sliding out: `ui` animates it and hides the window at the end.
    }

    /// Work area height in points.
    fn work_h(&self) -> f32 {
        (self.screen.bottom - self.screen.top) as f32 / self.scale
    }

    /// Panel height the shelf is heading to.
    fn target_h(&self) -> f32 {
        layout::panel_height(self.target_rows() as f32, self.work_h())
    }

    /// Panel height the window has room for.
    fn window_h(&self, now: Instant) -> f32 {
        self.height.room(now, self.animate)
    }

    /// Sizes the window and puts it at the configured place.
    fn place(&mut self, now: Instant) {
        let window_h = self.window_h(now);
        let size = layout::window_size(window_h);
        let rect = shelf_rect(self.cfg.edge, self.cfg.align, self.screen, size, self.scale);
        // winit rewrites the window styles on its own flag changes.
        shelve_platform::prepare_shelf_window(self.hwnd);
        shelve_platform::show_shelf_window(self.hwnd, rect);
        self.placed_h = Some(window_h);
        self.win = rect;
    }

    fn hide(&mut self) {
        shelve_platform::hide_shelf_window(self.hwnd);
        self.visible = false;
        self.placed_h = None;
        *self.shared.lock().unwrap() = None;
        self.sync_tab();
        // Once per hide, not per frame. The settings window is still being drawn and
        // would pull its pages straight back.
        if !self.settings.is_open() {
            shelve_platform::trim_memory();
        }
    }

    /// Re-reads theme and accent from Windows; restyles egui only if they changed.
    fn refresh_look(&mut self, ctx: &egui::Context) {
        theme::set_animations(ctx, shelve_platform::animations_enabled());
        let look = shelve_platform::system_look();
        if look != self.look {
            self.look = look;
            self.palette = theme::apply(ctx, look);
        }
    }

    /// The tab shows while the shelf is hidden and has files, at the shelf's place on the
    /// monitor it was last shown on, in the shelf's colours. Tells the platform thread
    /// only when that changes, a Windows theme change included.
    fn sync_tab(&mut self) {
        let want = (!self.visible && !self.shelf.items().is_empty()).then(|| TabSpec {
            rect: tab_rect(self.cfg.edge, self.cfg.align, self.screen, self.scale),
            edge: self.cfg.edge,
            scale: self.scale,
            colors: tab_colors(&self.palette),
        });
        if want != self.tab {
            self.tab = want;
            if let Some(p) = &self.platform {
                p.set_tab(want);
            }
        }
    }

    fn command(&self, command: Command) {
        let _ = self.auto_show.send(AutoShowInput::Command(command));
    }

    fn hotkey_status(&self) -> Option<HotkeyStatus> {
        self.platform.as_ref().map(|p| p.hotkey_status().clone())
    }

    fn apply(&mut self, changes: Changes) {
        let mut autostart_error = None;
        if let Err(e) = self.cfg.save(&default_config_path()) {
            println!("config save failed: {e}");
        }
        if changes.hook {
            self.mouse_hook = None; // joins the old hook thread first
            self.mouse_hook = start_mouse_hook(&self.cfg, self.auto_show.clone());
        }
        if changes.hotkey || changes.lang {
            self.platform = None; // releases the old hotkey before registering the new one
            self.platform = shelve_platform::start(&self.cfg, self.platform_tx.clone())
                .inspect_err(|e| println!("platform restart failed: {e}"))
                .ok();
            // The new thread's tab starts hidden; place it again.
            self.tab = None;
        }
        if changes.updates {
            self.updates.set_enabled(self.cfg.check_updates);
        }
        if changes.autostart {
            autostart_error = shelve_platform::set_autostart(self.cfg.autostart)
                .err()
                .map(|e| self.cfg.lang.autostart_failed(&e.to_string()));
        }
        self.settings
            .set_feedback(self.hotkey_status(), autostart_error);
        self.sync_tab(); // edge or alignment may have moved the tab too
        if self.visible {
            // Edge or alignment may have moved it. On the monitor it is on: `set_visible`
            // would take it to the one under the cursor (the settings window's).
            self.place(Instant::now());
        }
    }

    fn icon(&mut self, ctx: &egui::Context, item: &ShelfItem) -> Option<egui::TextureHandle> {
        self.icons
            .entry(item.path.clone())
            .or_insert_with(|| {
                let (w, h, rgba) =
                    shelve_platform::file_icon(&item.path, item.kind == ItemKind::Dir)?;
                let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                Some(ctx.load_texture(
                    item.path.to_string_lossy(),
                    img,
                    egui::TextureOptions::LINEAR,
                ))
            })
            .clone()
    }
}

impl eframe::App for ShelfApp {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    // Runs even while the window is hidden, whenever the platform thread calls request_repaint.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for a in self.presence_answers.try_iter() {
            if presence::remove_after_drop(a.after_drop, a.availability) {
                // Explorer moved it away; the shelf deletes nothing itself.
                println!("moved away: {}", a.path.display());
                self.shelf.remove(&a.path);
                self.presence.remove(&a.path);
            } else {
                self.presence.insert(a.path, a.availability);
            }
        }
        while let Ok(ev) = self.events.try_recv() {
            println!("event {ev:?}");
            // Every way to show the shelf arrives here as an event, so it never opens stale.
            self.refresh_look(ctx);
            match ev {
                // Asked for, not done here: the auto_show thread decides and answers.
                PlatformEvent::Hotkey | PlatformEvent::TrayToggle => self.command(Command::Toggle),
                PlatformEvent::ShowExisting => self.command(Command::ShowExisting),
                PlatformEvent::TabHovered => self.command(Command::TabHovered),
                PlatformEvent::EdgeShow => self.set_visible(true),
                PlatformEvent::EdgeHide => self.set_visible(false),
                PlatformEvent::TrayClear => self.shelf.clear(),
                PlatformEvent::TraySettings => self.settings.open(&self.cfg, self.hotkey_status()),
                PlatformEvent::TrayQuit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                PlatformEvent::ThemeChanged => ctx.request_repaint(),
            }
        }
        // After TrayClear or a moved-away file the hidden shelf may be empty: no tab then.
        self.sync_tab();
        if self.settings.is_open() && !self.settings.exists(ctx) {
            // With the shelf hidden `ui` would not run to create the settings window. Only
            // until it exists: then it keeps `ui` running itself, and asking every pass loops.
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(button) = self.release_pointer.take() {
            let pos = ctx.input(|i| i.pointer.latest_pos()).unwrap_or_default();
            raw_input.events.push(egui::Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers: Default::default(),
            });
            raw_input.events.push(egui::Event::PointerGone);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some((cfg, changes)) = self.settings.take_changes() {
            self.cfg = cfg;
            self.apply(changes);
        }
        if self.settings.is_open() {
            self.settings
                .show(ui.ctx(), self.palette, self.updates.found());
        }
        if !self.visible {
            // eframe shows the window after its first frame; its WM_PAINT brings us here to hide it.
            shelve_platform::hide_shelf_window(self.hwnd);
            return;
        }
        // Slide in or out from the edge; repaints stop once it settles.
        let done = self.slide_t() >= 1.0;
        if done && !self.target {
            self.hide();
            return;
        }
        let now = Instant::now();
        self.file_over = ui.input(|i| !i.raw.hovered_files.is_empty());
        // Taken before the rows are synced, so a dropped file gets its row in this very
        // frame: otherwise one frame shows the shelf without it.
        // winit's drop target only accepts CF_HDROP, so text and URLs never get here.
        let dropped: Vec<PathBuf> = ui.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            let (n, before) = (dropped.len(), self.shelf.items().len());
            println!(
                "dropped {n}, added {}",
                accept_drop(&mut self.shelf, dropped)
            );
            // New items go to the end. Present until the answer says otherwise.
            for item in &self.shelf.items()[before..] {
                self.presence.remove(&item.path);
                self.presence_queue.check(item.path.clone());
            }
        }
        self.rows.sync(self.shelf.items(), now, self.animate);
        let target = self.target_h();
        if target != self.height.to() {
            // Items came or went: glide to the new height from wherever the panel is.
            self.height.retarget(target, now, self.animate);
        }
        if self.placed_h != Some(self.window_h(now)) {
            // This frame is still drawn at the old window size.
            self.place(now);
            ui.ctx().request_repaint();
        }
        let panel_h = self.height.value(now, self.animate).round();
        let window_h = self.placed_h.unwrap_or(panel_h);
        let panel = layout::panel_rect_in(
            self.win,
            self.cfg.edge,
            self.cfg.align,
            window_h,
            panel_h,
            self.scale,
        );
        *self.shared.lock().unwrap() = Some(layout::hover_rect(panel, self.win, self.cfg.edge));
        if !done || self.rows.animating(now) || !self.height.settled(now, self.animate) {
            ui.ctx().request_repaint();
        }
        // The whole window, shadow room included, goes behind the edge.
        let away = self.offset() * layout::window_size(0.0).0;
        let dx = match self.cfg.edge {
            Edge::Right => away,
            Edge::Left => -away,
        };
        slide(ui, dx, |ui| self.shelf_ui(ui, now, window_h, panel_h));
    }
}

/// Draws `contents` shifted by `dx` points by moving the area it is laid out in. Not a layer
/// transform: egui keeps those per LayerId for every window (the shared background layer
/// shifted the settings window too), and a layer of our own gets no clicks.
fn slide<R>(ui: &mut egui::Ui, dx: f32, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let rect = ui.max_rect().translate(egui::vec2(dx, 0.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), contents)
        .inner
}

impl ShelfApp {
    /// Rows the panel is sized for: the items, plus the drop placeholder while a
    /// file is carried over a shelf that already has items.
    fn target_rows(&self) -> usize {
        let n = self.shelf.items().len();
        n + usize::from(self.file_over && n > 0)
    }

    /// `window_h`: panel height the window has room for; `panel_h`: the panel's height now.
    fn shelf_ui(&mut self, ui: &mut egui::Ui, now: Instant, window_h: f32, panel_h: f32) {
        // Nothing on the shelf is text to select; it is all drag handles and buttons.
        ui.style_mut().interaction.selectable_labels = false;
        // The panel inside the (possibly taller) window: a gap from the screen edge,
        // anchored like the shelf itself while its height glides.
        let left = match self.cfg.edge {
            Edge::Left => layout::GAP,
            Edge::Right => layout::SHADOW,
        };
        let top = layout::SHADOW + layout::panel_top_offset(self.cfg.align, window_h, panel_h);
        let panel_rect = egui::Rect::from_min_size(
            ui.max_rect().min + egui::vec2(left, top),
            egui::vec2(layout::PANEL_W, panel_h),
        );
        let panel = egui::Frame::new()
            // Opaque: a translucent shelf let the window under it show through.
            .fill(self.palette.bg)
            .corner_radius(layout::RADIUS)
            // No stroke here: a frame's stroke takes room, and the content would be
            // 2 pt shorter than the panel. The border is painted over it below.
            .shadow(egui::Shadow {
                offset: [0, 4],
                blur: 12,
                spread: 0,
                color: self.palette.shadow,
            });
        let ctx = ui.ctx().clone();
        let (palette, file_over) = (self.palette, self.file_over);
        // Lent out for the pass: the view reads the rows while `icon` borrows `self`.
        let rows = std::mem::take(&mut self.rows);
        let action = ui
            .scope_builder(egui::UiBuilder::new().max_rect(panel_rect), |ui| {
                panel
                    .show(ui, |ui| {
                        ui.set_min_size(ui.available_size());
                        let look = (&palette, file_over, self.cfg.lang);
                        // Lent too: `icon` borrows `self` mutably.
                        let presence = std::mem::take(&mut self.presence);
                        let action = shelf_view(ui, &rows, now, look, &presence, &mut |item| {
                            self.icon(&ctx, item)
                        });
                        self.presence = presence;
                        action
                    })
                    .inner
            })
            .inner;
        self.rows = rows;
        if !file_over {
            // With a file over the shelf the view draws its own accent border.
            ui.painter().rect_stroke(
                panel_rect,
                layout::RADIUS,
                egui::Stroke::new(1.0, self.palette.border),
                egui::StrokeKind::Inside,
            );
        }
        if action.is_some() {
            ui.ctx().request_repaint(); // the change shows from the next frame on
        }
        match action {
            Some(Action::Remove(path)) => {
                self.shelf.remove(&path);
            }
            Some(Action::Clear) => self.shelf.clear(),
            Some(Action::OpenSettings) => self.settings.open(&self.cfg, self.hotkey_status()),
            Some(Action::DragOut(item, button)) => {
                let paths = drag_candidates(std::slice::from_ref(&item), &self.presence);
                if !paths.is_empty() {
                    let res = shelve_platform::start_drag_out(&paths, button);
                    println!("drag_out {button:?} {res:?}");
                    self.release_pointer = Some(match button {
                        DragButton::Left => egui::PointerButton::Primary,
                        DragButton::Right => egui::PointerButton::Secondary,
                    });
                    ui.ctx().request_repaint();
                    match res {
                        // Explorer may move it only later: watch until it goes or time is up.
                        Ok(DragOutcome::Dropped) => self.presence_queue.watch(item.path),
                        _ => self.presence_queue.check(item.path),
                    }
                }
            }
            None => {}
        }
    }
}

/// The tab is a piece of the shelf: its background and border, the arrow in the accent.
fn tab_colors(p: &Palette) -> TabColors {
    let rgb = |c: egui::Color32| [c.r(), c.g(), c.b()];
    TabColors {
        bg: rgb(p.bg),
        border: rgb(p.border),
        accent: rgb(p.accent),
    }
}

/// Adds only paths that exist; returns how many were actually added.
pub fn accept_drop(shelf: &mut Shelf, paths: Vec<PathBuf>) -> usize {
    paths
        .into_iter()
        .filter(|p| p.exists())
        .filter(|p| shelf.add(p.clone()))
        .count()
}

/// Paths of the items not known to be gone; missing ones are never dragged out.
/// Reads `presence` only: the disk is not asked on the UI thread.
pub fn drag_candidates(items: &[ShelfItem], presence: &Presence) -> Vec<PathBuf> {
    items
        .iter()
        .filter(|i| !is_missing(presence, &i.path))
        .map(|i| i.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shelve_core::Availability;

    /// Runs a pass with `events`; returns whether the button inside `slide` was clicked.
    fn click_pass(ctx: &egui::Context, dx: f32, events: Vec<egui::Event>) -> bool {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(220.0, 400.0),
            )),
            events,
            ..Default::default()
        };
        let mut clicked = false;
        let mut out = ctx.run_ui(input, |ui| {
            slide(ui, dx, |ui| clicked = ui.button("Clear").clicked());
        });
        out.textures_delta.clear();
        clicked
    }

    #[test]
    fn widgets_inside_slide_take_clicks() {
        // Bug: nothing on the shelf could be clicked or dragged after it moved to its own layer.
        let ctx = egui::Context::default();
        click_pass(&ctx, 0.0, vec![]); // lay out once to know where the button is
        let at = egui::pos2(20.0, 10.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        click_pass(&ctx, 0.0, vec![egui::Event::PointerMoved(at), button(true)]);
        assert!(click_pass(&ctx, 0.0, vec![button(false)]));
    }

    #[test]
    fn slide_does_not_move_other_windows() {
        // Bug: the settings window was shifted by the hidden shelf's slide offset. egui keeps
        // layer transforms per LayerId for all windows, and the shelf moved the shared
        // background layer.
        let ctx = egui::Context::default();
        let mut panel_left = 0.0;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            slide(ui, 100.0, |ui| {
                egui::CentralPanel::default().show(ui, |ui| panel_left = ui.max_rect().left());
            });
        });
        out.textures_delta.clear();
        assert_eq!(
            ctx.layer_transform_to_global(egui::LayerId::background()),
            None
        );
        assert!(
            panel_left >= 100.0,
            "panel did not move: left = {panel_left}"
        );
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shelve-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn drag_candidates_skips_the_ones_marked_missing() {
        // Only the presence answers count, the disk is not asked: both paths exist.
        let (a, b) = (std::env::temp_dir(), PathBuf::from(r"C:\"));
        let mut shelf = Shelf::new();
        shelf.add(a.clone());
        shelf.add(b.clone());
        let presence = Presence::from([(b, Availability::Missing)]);
        assert_eq!(drag_candidates(shelf.items(), &presence), vec![a]);
    }

    #[test]
    fn drag_candidates_take_unchecked_paths() {
        let mut shelf = Shelf::new();
        shelf.add(PathBuf::from(r"C:\shelve-нет\a.txt"));
        let all = vec![PathBuf::from(r"C:\shelve-нет\a.txt")];
        assert_eq!(drag_candidates(shelf.items(), &Presence::new()), all);
    }

    #[test]
    fn accept_drop_adds_existing_only() {
        let dir = temp_dir("existing");
        let a = dir.join("а файл.txt");
        std::fs::write(&a, b"x").unwrap();
        let mut shelf = Shelf::new();
        let added = accept_drop(&mut shelf, vec![a, dir.clone(), dir.join("нет такого.txt")]);
        assert_eq!(added, 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn accept_drop_skips_duplicates() {
        let dir = temp_dir("dupes");
        let mut shelf = Shelf::new();
        assert_eq!(accept_drop(&mut shelf, vec![dir.clone(), dir.clone()]), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn accept_drop_handles_1000_files() {
        let dir = temp_dir("many");
        let paths: Vec<PathBuf> = (0..1000)
            .map(|i| {
                let p = dir.join(format!("{i}.txt"));
                std::fs::write(&p, b"x").unwrap();
                p
            })
            .collect();
        let mut shelf = Shelf::new();
        assert_eq!(accept_drop(&mut shelf, paths), 1000);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
