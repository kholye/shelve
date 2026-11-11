//! What is drawn inside the shelf panel: header, rows, the empty plate and the drop
//! highlight. Reports what the user did as an [`Action`]; changes nothing itself.

use std::time::Instant;

use eframe::egui::{self, pos2, vec2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke};
use eframe::epaint::text::{LayoutJob, TextFormat, TextWrapping};
use shelve_core::{ItemKind, Lang, ShelfItem, Text};

use crate::labels::folder_label;
use crate::layout::{HEADER_H, PAD_BOTTOM, RADIUS, ROW_H};
use crate::presence::{is_missing, Presence};
use crate::rows::Rows;
use crate::theme::Palette;

/// Side of the square icon buttons (gear, remove).
const BUTTON: f32 = 28.0;
const ICON: f32 = 32.0;
/// Rows keep this far from the panel sides.
const ROW_MARGIN: f32 = 4.0;
const ROW_RADIUS: f32 = 6.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Remove(std::path::PathBuf),
    Clear,
    OpenSettings,
    /// The user started dragging this row out of the shelf.
    DragOut(ShelfItem),
}

/// Draws the shelf into `ui` (the inside of the panel). `rows` are the items of the
/// shelf with their animation state at `now`. `file_over`: a file is being carried over
/// the shelf. `presence` says which files are gone; the view never asks the disk itself.
/// `icon` returns the shell icon of an item, if there is one.
pub fn shelf_view(
    ui: &mut egui::Ui,
    rows: &Rows,
    now: Instant,
    (p, file_over, lang): (&Palette, bool, Lang),
    presence: &Presence,
    icon: &mut dyn FnMut(&ShelfItem) -> Option<egui::TextureHandle>,
) -> Option<Action> {
    let panel = ui.max_rect();
    let mut action = None;
    let t = lang.text();
    if rows.rows().is_empty() {
        empty_plate(ui, panel, p, file_over, t);
    } else {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        // Rows on their way out are no longer on the shelf.
        let count = rows.rows().iter().filter(|r| !rows.is_leaving(r)).count();
        action = header(ui, header_rect(panel), &lang.count(count), p, t);
        let list = Rect::from_min_max(
            pos2(panel.left(), panel.top() + HEADER_H),
            pos2(panel.right(), panel.bottom() - PAD_BOTTOM),
        );
        let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list));
        // While a row collapses and the panel shrinks with it, the content can be a
        // fraction of a point taller than the list. That is not a reason to scroll.
        let content: f32 = rows
            .rows()
            .iter()
            .map(|r| ROW_H * rows.height(r, now))
            .sum();
        let ghost = if file_over { ROW_H } else { 0.0 };
        let scrolls = content + ghost > list.height() + 1.0;
        egui::ScrollArea::vertical()
            .scroll_bar_visibility(if scrolls {
                egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
            } else {
                egui::scroll_area::ScrollBarVisibility::AlwaysHidden
            })
            // Full width and height even with few short names: otherwise the scroll
            // bar sticks to the text instead of the panel edge.
            .auto_shrink([false, false])
            .show(&mut list_ui, |ui| {
                for r in rows.rows() {
                    let look = RowLook {
                        height: rows.height(r, now),
                        flash: rows.flash(r, now),
                        interactive: !rows.is_leaving(r),
                        missing: is_missing(presence, &r.item.path),
                    };
                    let size = vec2(ui.available_width(), ROW_H * look.height);
                    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                    if look.height <= 0.0 || !ui.is_rect_visible(rect) {
                        continue; // hundreds of items: only the visible ones cost anything
                    }
                    let tex = icon(&r.item);
                    action = row(ui, rect, &r.item, tex, p, look, t).or(action.take());
                }
                if file_over {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
                    drop_placeholder(ui, rect, p, t);
                    ui.scroll_to_rect(rect, None);
                }
            });
    }
    if file_over {
        ui.painter().rect_stroke(
            panel,
            RADIUS,
            Stroke::new(2.0, p.accent),
            egui::StrokeKind::Inside,
        );
    }
    action
}

fn header_rect(panel: Rect) -> Rect {
    Rect::from_min_size(panel.min, vec2(panel.width(), HEADER_H))
}

fn gear_rect(header: Rect) -> Rect {
    Rect::from_center_size(
        pos2(header.right() - 6.0 - BUTTON / 2.0, header.center().y),
        vec2(BUTTON, BUTTON),
    )
}

/// The part of a row that is drawn and takes clicks.
fn row_body(row: Rect) -> Rect {
    row.shrink2(vec2(ROW_MARGIN, 0.0))
}

fn remove_rect(row: Rect) -> Rect {
    let body = row_body(row);
    Rect::from_center_size(
        pos2(body.right() - 6.0 - BUTTON / 2.0, body.center().y),
        vec2(BUTTON, BUTTON),
    )
}

fn header(ui: &mut egui::Ui, rect: Rect, count: &str, p: &Palette, t: &Text) -> Option<Action> {
    let small = FontId::proportional(12.0);
    ui.painter().text(
        pos2(rect.left() + 12.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        count,
        small.clone(),
        p.text_weak,
    );

    let gear = gear_rect(rect);
    let gear_resp = ui
        .interact(gear, ui.scope_id().with("settings"), Sense::click())
        .on_hover_text(t.settings);
    gear_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t.settings));
    if gear_resp.hovered() {
        ui.painter().rect_filled(gear, 4.0, p.hover);
    }
    paint_gear(ui.painter(), gear.center(), p.text_weak);

    let label = ui
        .painter()
        .layout_no_wrap(t.clear.to_owned(), small, p.accent);
    let clear = Rect::from_min_max(
        pos2(gear.left() - 2.0 - label.size().x - 16.0, gear.top()),
        pos2(gear.left() - 2.0, gear.bottom()),
    );
    let clear_resp = ui.interact(clear, ui.scope_id().with("clear"), Sense::click());
    clear_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t.clear));
    if clear_resp.hovered() {
        ui.painter().rect_filled(clear, 4.0, p.hover);
    }
    ui.painter()
        .galley(clear.center() - label.size() / 2.0, label, p.accent);

    if gear_resp.clicked() {
        Some(Action::OpenSettings)
    } else if clear_resp.clicked() {
        Some(Action::Clear)
    } else {
        None
    }
}

/// How a row is drawn while it grows in, flashes or collapses.
#[derive(Clone, Copy)]
struct RowLook {
    /// Share of the full height, also used as opacity. 1: at rest.
    height: f32,
    /// Highlight of a fresh row, 1..0.
    flash: f32,
    /// False for a row on its way out: it no longer reacts to the mouse.
    interactive: bool,
    /// The file is gone: greyed and struck through, a click removes the row.
    missing: bool,
}

fn row(
    ui: &mut egui::Ui,
    rect: Rect,
    item: &ShelfItem,
    tex: Option<egui::TextureHandle>,
    p: &Palette,
    look: RowLook,
    t: &Text,
) -> Option<Action> {
    let body = row_body(rect);
    // A row that is not at full height shows a cut-off slice of itself.
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let fade = |c: Color32| c.gamma_multiply(look.height);
    let (text_color, weak) = (fade(p.text), fade(p.text_weak));
    let missing = look.missing;
    let id = ui.scope_id().with(&item.path);
    // The whole row is the handle: click (to dismiss a missing file) and drag out.
    let resp = look.interactive.then(|| {
        ui.interact(body, id, Sense::click_and_drag())
            .on_hover_text(item.path.display().to_string())
    });
    // Not `resp.hovered()`: that is false while the pointer is on the remove button.
    let under_pointer = look.interactive && ui.rect_contains_pointer(body);
    if look.flash > 0.0 {
        let wash = p.accent_bg.gamma_multiply(look.flash * look.height);
        painter.rect_filled(body, ROW_RADIUS, wash);
    }
    if under_pointer {
        painter.rect_filled(body, ROW_RADIUS, p.hover);
    }

    let icon_rect = Rect::from_center_size(
        pos2(body.left() + 8.0 + ICON / 2.0, body.center().y),
        vec2(ICON, ICON),
    );
    match tex {
        _ if missing => dashed_rect(
            &painter,
            icon_rect.shrink2(vec2(3.0, 0.0)),
            4.0,
            Stroke::new(1.0, weak),
        ),
        Some(tex) => {
            let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(tex.id(), icon_rect, whole, fade(Color32::WHITE));
        }
        None => paint_kind(&painter, icon_rect, item.kind, weak),
    }

    let text_left = icon_rect.right() + 10.0;
    let text_right = if under_pointer {
        remove_rect(rect).left() - 4.0
    } else {
        body.right() - 8.0
    };
    let width = (text_right - text_left).max(0.0);
    let name = item
        .path
        .file_name()
        .unwrap_or(item.path.as_os_str())
        .to_string_lossy()
        .into_owned();
    let name_color = if missing { weak } else { text_color };
    let name_format = TextFormat {
        font_id: FontId::proportional(13.0),
        color: name_color,
        strikethrough: if missing {
            Stroke::new(1.0, name_color)
        } else {
            Stroke::NONE
        },
        ..Default::default()
    };
    let second = if missing {
        t.missing.to_owned()
    } else {
        folder_label(&item.path)
    };
    let second_format = TextFormat {
        font_id: FontId::proportional(11.0),
        color: weak,
        ..Default::default()
    };
    let name = one_line(ui, name, name_format, width);
    let second = one_line(ui, second, second_format, width);
    let top = body.center().y - (name.size().y + second.size().y) / 2.0;
    let second_top = top + name.size().y;
    painter.galley(pos2(text_left, top), name, name_color);
    painter.galley(pos2(text_left, second_top), second, weak);

    let resp = resp?;
    let mut remove = missing && resp.clicked();
    if under_pointer {
        // Added after the row, so it is on top and gets the click.
        let x = remove_rect(rect);
        let x_resp = ui
            .interact(x, id.with("remove"), Sense::click())
            .on_hover_text(t.remove);
        x_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, t.remove));
        if x_resp.hovered() {
            painter.rect_filled(x, 4.0, p.border);
        }
        let (c, d, stroke) = (x.center(), 4.0, Stroke::new(1.4, weak));
        painter.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
        painter.line_segment([c + vec2(d, -d), c + vec2(-d, d)], stroke);
        remove |= x_resp.clicked();
    }

    if remove {
        Some(Action::Remove(item.path.clone()))
    } else if resp.drag_started() && !missing {
        Some(Action::DragOut(item.clone()))
    } else {
        None
    }
}

/// One line of text, cut with an ellipsis at `width`.
fn one_line(
    ui: &egui::Ui,
    text: String,
    format: TextFormat,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::single_section(text, format);
    job.wrap = TextWrapping::truncate_at_width(width);
    ui.painter().layout_job(job)
}

fn drop_placeholder(ui: &egui::Ui, row: Rect, p: &Palette, t: &Text) {
    let body = row_body(row).shrink2(vec2(0.0, 2.0));
    ui.painter().rect_filled(body, ROW_RADIUS, p.accent_bg);
    dashed_rect(ui.painter(), body, ROW_RADIUS, Stroke::new(1.0, p.accent));
    ui.painter().text(
        body.center(),
        egui::Align2::CENTER_CENTER,
        t.drop_hint,
        FontId::proportional(12.0),
        p.text_weak,
    );
}

fn empty_plate(ui: &egui::Ui, panel: Rect, p: &Palette, file_over: bool, t: &Text) {
    let plate = Rect::from_min_size(
        pos2(panel.center().x - 36.0, panel.top() + 22.0),
        vec2(72.0, 56.0),
    );
    let color = if file_over { p.accent } else { p.text_weak };
    if file_over {
        ui.painter().rect_filled(plate, RADIUS, p.accent_bg);
    }
    dashed_rect(ui.painter(), plate, RADIUS, Stroke::new(1.5, color));
    paint_shelf_glyph(
        ui.painter(),
        Rect::from_center_size(plate.center(), vec2(34.0, 34.0)),
        color,
    );
    let text = if file_over { t.drop_hint } else { t.empty_hint };
    ui.painter().text(
        pos2(panel.center().x, plate.bottom() + 12.0),
        egui::Align2::CENTER_TOP,
        text,
        FontId::proportional(13.0),
        p.text_weak,
    );
}

/// The app icon's drawing (shelf, two books, a leaning third) inside `rect`.
fn paint_shelf_glyph(painter: &egui::Painter, rect: Rect, color: Color32) {
    let s = rect.width() / 32.0;
    let at = |x: f32, y: f32| rect.min + vec2(x, y) * s;
    let bar = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x, y), vec2(w, h) * s);
    painter.rect_filled(bar(4.0, 23.0, 24.0, 3.5), 1.75 * s, color);
    painter.rect_filled(bar(6.0, 8.0, 5.0, 13.5), 1.2 * s, color);
    painter.rect_filled(bar(12.5, 11.0, 5.0, 10.5), 1.2 * s, color);
    // Third book: 5 x 13.5 standing on (23, 22), leaning 20 degrees to the left.
    let (sin, cos) = 20f32.to_radians().sin_cos();
    let turned = |dx: f32, dy: f32| at(23.0 + dx * cos + dy * sin, 22.0 - dx * sin + dy * cos);
    painter.add(Shape::convex_polygon(
        vec![
            turned(-2.5, -13.5),
            turned(2.5, -13.5),
            turned(2.5, 0.0),
            turned(-2.5, 0.0),
        ],
        color,
        Stroke::NONE,
    ));
}

/// Gear: a ring and eight short spokes, in a 16-point box around `c`.
fn paint_gear(painter: &egui::Painter, c: Pos2, color: Color32) {
    let stroke = Stroke::new(1.3, color);
    painter.circle_stroke(c, 2.2, stroke);
    for i in 0..8 {
        let (sin, cos) = (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
        let dir = vec2(cos, sin);
        painter.line_segment([c + dir * 4.5, c + dir * 6.5], stroke);
    }
}

/// Drawn file/folder glyph (never emoji), used when the shell gives no icon.
fn paint_kind(painter: &egui::Painter, rect: Rect, kind: ItemKind, color: Color32) {
    match kind {
        ItemKind::Dir => {
            let body = Rect::from_min_max(rect.min + vec2(2.0, 9.0), rect.max - vec2(2.0, 4.0));
            let tab = Rect::from_min_size(rect.min + vec2(2.0, 5.0), vec2(12.0, 6.0));
            painter.rect_filled(tab, 2.0, color);
            painter.rect_filled(body, 3.0, color);
        }
        ItemKind::File => {
            painter.rect_stroke(
                rect.shrink2(vec2(5.0, 2.0)),
                2.0,
                Stroke::new(1.5, color),
                egui::StrokeKind::Inside,
            );
        }
    }
}

/// Dashed outline of a rounded rectangle.
fn dashed_rect(painter: &egui::Painter, rect: Rect, radius: f32, stroke: Stroke) {
    const ARC_STEPS: usize = 6;
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let inner = rect.shrink(r);
    // Corner centres clockwise from top-left, each with the angle its arc starts at.
    let corners = [
        (inner.left_top(), 180f32),
        (inner.right_top(), 270.0),
        (inner.right_bottom(), 0.0),
        (inner.left_bottom(), 90.0),
    ];
    let mut path = Vec::with_capacity(4 * (ARC_STEPS + 1) + 1);
    for (centre, start) in corners {
        for step in 0..=ARC_STEPS {
            let angle = (start + 90.0 * step as f32 / ARC_STEPS as f32).to_radians();
            path.push(centre + vec2(angle.cos(), angle.sin()) * r);
        }
    }
    path.push(path[0]);
    painter.extend(Shape::dashed_line(&path, stroke, 4.0, 3.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PANEL_W;
    use crate::theme::palette;
    use shelve_core::Availability;
    use std::path::PathBuf;

    const PANEL: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(PANEL_W, 400.0));

    fn existing(n: usize) -> Vec<ShelfItem> {
        // Real paths, so a test can show the view ignores the disk. Their names differ, the
        // test never opens them.
        let dir = std::env::temp_dir();
        std::iter::once(dir.clone())
            .chain(dir.ancestors().skip(1).map(PathBuf::from))
            .take(n)
            .map(|path| ShelfItem {
                path,
                kind: ItemKind::Dir,
            })
            .collect()
    }

    fn missing() -> ShelfItem {
        ShelfItem {
            path: PathBuf::from(r"C:\shelve-нет\a.txt"),
            kind: ItemKind::File,
        }
    }

    fn row_rect(i: usize) -> Rect {
        Rect::from_min_size(
            pos2(PANEL.left(), PANEL.top() + HEADER_H + i as f32 * ROW_H),
            vec2(PANEL.width(), ROW_H),
        )
    }

    /// What the presence checker would have answered: `missing()` is gone.
    fn presence() -> Presence {
        Presence::from([(missing().path, Availability::Missing)])
    }

    fn pass(
        ctx: &egui::Context,
        items: &[ShelfItem],
        file_over: bool,
        events: Vec<egui::Event>,
    ) -> Option<Action> {
        pass_with(ctx, items, &presence(), file_over, events)
    }

    fn pass_with(
        ctx: &egui::Context,
        items: &[ShelfItem],
        presence: &Presence,
        file_over: bool,
        events: Vec<egui::Event>,
    ) -> Option<Action> {
        let input = egui::RawInput {
            screen_rect: Some(PANEL),
            events,
            ..Default::default()
        };
        let p = palette(false, [0x0F, 0x6C, 0xBD]);
        let mut action = None;
        let now = Instant::now();
        let mut rows = Rows::default();
        rows.sync(items, now, false);
        let mut out = ctx.run_ui(input, |ui| {
            let look = (&p, file_over, Lang::En);
            action = shelf_view(ui, &rows, now, look, presence, &mut |_| None);
        });
        out.textures_delta.clear();
        action
    }

    #[test]
    fn a_row_marked_missing_is_drawn_missing_though_its_path_exists() {
        // The view never asks the disk: a sleeping network drive froze every frame.
        let items = existing(1);
        let marked = Presence::from([(items[0].path.clone(), Availability::Missing)]);
        let at = row_rect(0).left_center() + vec2(60.0, 0.0);
        let ctx = egui::Context::default();
        pass_with(&ctx, &items, &marked, false, vec![]);
        pass_with(
            &ctx,
            &items,
            &marked,
            false,
            vec![egui::Event::PointerMoved(at)],
        );
        pass_with(&ctx, &items, &marked, false, vec![button(at, true)]);
        let released = pass_with(&ctx, &items, &marked, false, vec![button(at, false)]);
        assert_eq!(released, Some(Action::Remove(items[0].path.clone())));
    }

    #[test]
    fn a_path_not_yet_checked_counts_as_present() {
        let at = row_rect(0).left_center() + vec2(60.0, 0.0);
        let ctx = egui::Context::default();
        let none = Presence::new();
        let items = [missing()];
        pass_with(&ctx, &items, &none, false, vec![]);
        pass_with(
            &ctx,
            &items,
            &none,
            false,
            vec![egui::Event::PointerMoved(at)],
        );
        pass_with(&ctx, &items, &none, false, vec![button(at, true)]);
        let released = pass_with(&ctx, &items, &none, false, vec![button(at, false)]);
        assert_eq!(released, None, "a present row ignores a click");
    }

    fn button(at: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    /// Hovers `at`, presses and releases there; returns what the release produced.
    fn click(items: &[ShelfItem], at: Pos2) -> Option<Action> {
        let ctx = egui::Context::default();
        pass(&ctx, items, false, vec![]);
        pass(&ctx, items, false, vec![egui::Event::PointerMoved(at)]);
        pass(&ctx, items, false, vec![button(at, true)]);
        pass(&ctx, items, false, vec![button(at, false)])
    }

    #[test]
    fn empty_shelf_draws_and_does_nothing() {
        let ctx = egui::Context::default();
        assert_eq!(pass(&ctx, &[], false, vec![]), None);
        assert_eq!(pass(&ctx, &[], true, vec![]), None);
    }

    #[test]
    fn gear_opens_settings() {
        let at = gear_rect(header_rect(PANEL)).center();
        assert_eq!(click(&existing(2), at), Some(Action::OpenSettings));
    }

    #[test]
    fn clear_link_left_of_the_gear_clears() {
        let at = gear_rect(header_rect(PANEL)).left_center() - vec2(20.0, 0.0);
        assert_eq!(click(&existing(2), at), Some(Action::Clear));
    }

    #[test]
    fn remove_button_at_the_right_end_of_a_row_removes_that_row() {
        // The button sits at the panel side whatever the length of the names.
        let items = existing(3);
        let at = remove_rect(row_rect(1)).center();
        assert!(at.x > PANEL.right() - 40.0);
        assert_eq!(
            click(&items, at),
            Some(Action::Remove(items[1].path.clone()))
        );
    }

    #[test]
    fn click_on_an_existing_row_does_nothing() {
        let at = row_rect(0).left_center() + vec2(60.0, 0.0);
        assert_eq!(click(&existing(2), at), None);
    }

    #[test]
    fn click_on_a_missing_row_removes_it() {
        let items = [existing(1).remove(0), missing()];
        let at = row_rect(1).left_center() + vec2(60.0, 0.0);
        assert_eq!(click(&items, at), Some(Action::Remove(missing().path)));
    }

    #[test]
    fn dragging_a_row_starts_a_drag_out() {
        let items = existing(2);
        let at = row_rect(1).left_center() + vec2(60.0, 0.0);
        let ctx = egui::Context::default();
        pass(&ctx, &items, false, vec![]);
        pass(&ctx, &items, false, vec![egui::Event::PointerMoved(at)]);
        pass(&ctx, &items, false, vec![button(at, true)]);
        let moved = egui::Event::PointerMoved(at - vec2(40.0, 0.0));
        assert_eq!(
            pass(&ctx, &items, false, vec![moved]),
            Some(Action::DragOut(items[1].clone()))
        );
    }

    #[test]
    fn a_missing_row_cannot_be_dragged_out() {
        let items = [missing()];
        let at = row_rect(0).left_center() + vec2(60.0, 0.0);
        let ctx = egui::Context::default();
        pass(&ctx, &items, false, vec![]);
        pass(&ctx, &items, false, vec![egui::Event::PointerMoved(at)]);
        pass(&ctx, &items, false, vec![button(at, true)]);
        let moved = egui::Event::PointerMoved(at - vec2(40.0, 0.0));
        assert_eq!(pass(&ctx, &items, false, vec![moved]), None);
    }

    #[test]
    fn hundreds_of_rows_with_a_file_over_them_draw() {
        let many: Vec<ShelfItem> = (0..500)
            .map(|i| ShelfItem {
                path: PathBuf::from(format!(r"C:\shelve-нет\{i}.txt")),
                kind: ItemKind::File,
            })
            .collect();
        let ctx = egui::Context::default();
        assert_eq!(pass(&ctx, &many, true, vec![]), None);
    }
}
