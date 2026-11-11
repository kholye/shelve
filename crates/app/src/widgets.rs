//! Small controls for the settings window, drawn by hand to look like Windows 11
//! Settings: a switch, a picture of the screen to pick the shelf's place on, a card.

use eframe::egui::{self, pos2, vec2, Rect, Sense, Stroke};
use shelve_core::{Align, Edge, Lang};

use crate::theme::Palette;

const TRACK: egui::Vec2 = vec2(40.0, 20.0);
const KNOB_R: f32 = 6.0;
/// The picture of the screen in the place picker.
const SCREEN_SIZE: egui::Vec2 = vec2(132.0, 84.0);
const SPOT: egui::Vec2 = vec2(14.0, 22.0);
/// From the picture's border to a spot.
const SPOT_INSET: f32 = 5.0;

/// A switch with its label on the left and the track on the right; the whole row
/// takes the click. `changed()` on the response: it was flipped in this pass.
pub fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
    let height = TRACK.y.max(ui.text_style_height(&egui::TextStyle::Body));
    let (rect, mut resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, true, *on, label));

    if ui.is_rect_visible(rect) {
        let v = ui.visuals();
        let (accent, weak, text) = (
            v.selection.stroke.color,
            v.weak_text_color(),
            v.text_color(),
        );
        let painter = ui.painter();
        painter.text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::TextStyle::Body.resolve(ui.style()),
            text,
        );
        let track = Rect::from_min_size(
            pos2(rect.right() - TRACK.x, rect.center().y - TRACK.y / 2.0),
            TRACK,
        );
        let radius = TRACK.y / 2.0;
        // The knob slides instead of jumping, unless Windows animations are off
        // (`theme::set_animations` sets egui's animation time to zero then).
        let t = ui.ctx().animate_bool_responsive(resp.id, *on);
        let knob_x = egui::lerp(track.left() + radius..=track.right() - radius, t);
        let knob = pos2(knob_x, track.center().y);
        if *on {
            painter.rect_filled(track, radius, accent);
            painter.circle_filled(knob, KNOB_R, v.panel_fill);
        } else {
            let outline = if resp.hovered() { text } else { weak };
            painter.rect_stroke(
                track,
                radius,
                Stroke::new(1.5, outline),
                egui::StrokeKind::Inside,
            );
            painter.circle_filled(knob, KNOB_R, outline);
        }
        if resp.has_focus() {
            painter.rect_stroke(
                track.expand(3.0),
                radius + 3.0,
                Stroke::new(1.5, text),
                egui::StrokeKind::Outside,
            );
        }
    }
    resp
}

/// "Right, center": the chosen place in words.
pub fn place_label(lang: Lang, edge: Edge, align: Align) -> String {
    let t = lang.text();
    let side = match edge {
        Edge::Left => t.left,
        Edge::Right => t.right,
    };
    let height = match align {
        Align::Top => t.top,
        Align::Center => t.center,
        Align::Bottom => t.bottom,
    };
    format!("{side}, {height}")
}

/// The language choice: every language under its own name, the current one filled.
/// Returns whether another one was picked.
pub fn lang_picker(ui: &mut egui::Ui, lang: &mut Lang) -> bool {
    let before = *lang;
    // Laid out from the right, so the list is walked backwards.
    for l in Lang::ALL.into_iter().rev() {
        // In the text colour: the accent the selection is filled with may be as dark.
        let name = egui::RichText::new(l.name()).color(if *lang == l {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().text_color()
        });
        ui.selectable_value(lang, l, name);
    }
    *lang != before
}

/// Where the spot for a place sits inside the picture of the screen.
fn spot_rect(screen: Rect, edge: Edge, align: Align) -> Rect {
    let x = match edge {
        Edge::Left => screen.left() + SPOT_INSET,
        Edge::Right => screen.right() - SPOT_INSET - SPOT.x,
    };
    let y = match align {
        Align::Top => screen.top() + SPOT_INSET,
        Align::Center => screen.center().y - SPOT.y / 2.0,
        Align::Bottom => screen.bottom() - SPOT_INSET - SPOT.y,
    };
    Rect::from_min_size(pos2(x, y), SPOT)
}

/// A picture of the screen with six places for the shelf; a click picks one.
/// Returns (edge changed, alignment changed).
pub fn place_picker(
    ui: &mut egui::Ui,
    lang: Lang,
    edge: &mut Edge,
    align: &mut Align,
) -> (bool, bool) {
    let (screen, _) = ui.allocate_exact_size(SCREEN_SIZE, Sense::hover());
    let v = ui.visuals().clone();
    let (accent, weak) = (v.selection.stroke.color, v.weak_text_color());
    ui.painter().rect_filled(screen, 6.0, v.faint_bg_color);
    ui.painter().rect_stroke(
        screen,
        6.0,
        Stroke::new(1.5, weak),
        egui::StrokeKind::Inside,
    );
    let before = (*edge, *align);
    for e in [Edge::Left, Edge::Right] {
        for a in [Align::Top, Align::Center, Align::Bottom] {
            let spot = spot_rect(screen, e, a);
            let selected = (e, a) == (*edge, *align);
            // A little larger than it looks: 14 x 22 is a small target.
            let resp = ui
                .interact(
                    spot.expand(3.0),
                    ui.scope_id().with(("place", e == Edge::Left, a as u8)),
                    Sense::click(),
                )
                .on_hover_text(place_label(lang, e, a));
            resp.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::RadioButton,
                    true,
                    selected,
                    place_label(lang, e, a),
                )
            });
            if resp.clicked() {
                (*edge, *align) = (e, a);
            }
            let painter = ui.painter();
            if selected {
                painter.rect_filled(spot, 3.0, accent);
            } else {
                let outline = if resp.hovered() { accent } else { weak };
                painter.rect_stroke(
                    spot,
                    3.0,
                    Stroke::new(1.5, outline),
                    egui::StrokeKind::Inside,
                );
            }
            if resp.has_focus() {
                painter.rect_stroke(
                    spot.expand(2.0),
                    4.0,
                    Stroke::new(1.5, v.text_color()),
                    egui::StrokeKind::Outside,
                );
            }
        }
    }
    (*edge != before.0, *align != before.1)
}

/// A bordered block as wide as the window, like a card in Windows Settings.
pub fn card<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(card_fill(p, ui.visuals().dark_mode))
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Cards stand out from the window: white on the light grey window, the palette's
/// own grey on the darker window of the dark theme.
fn card_fill(p: &Palette, dark: bool) -> egui::Color32 {
    if dark {
        p.bg
    } else {
        egui::Color32::WHITE
    }
}

/// Background of the settings window behind the cards.
pub fn window_fill(dark: bool) -> egui::Color32 {
    egui::Color32::from_gray(if dark { 0x20 } else { 0xF3 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Pos2;

    #[test]
    fn place_label_names_all_six_places() {
        let ru = |e, a| place_label(Lang::Ru, e, a);
        assert_eq!(ru(Edge::Right, Align::Center), "Справа, по центру");
        assert_eq!(ru(Edge::Right, Align::Top), "Справа, сверху");
        assert_eq!(ru(Edge::Right, Align::Bottom), "Справа, снизу");
        assert_eq!(ru(Edge::Left, Align::Top), "Слева, сверху");
        assert_eq!(ru(Edge::Left, Align::Center), "Слева, по центру");
        assert_eq!(ru(Edge::Left, Align::Bottom), "Слева, снизу");
        let en = |e, a| place_label(Lang::En, e, a);
        assert_eq!(en(Edge::Left, Align::Top), "Left, top");
        assert_eq!(en(Edge::Right, Align::Center), "Right, center");
    }

    fn pass(ctx: &egui::Context, events: Vec<egui::Event>, mut add: impl FnMut(&mut egui::Ui)) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| add(ui));
        out.textures_delta.clear();
    }

    fn button(at: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    fn key(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Default::default(),
        }
    }

    /// Runs passes that click at `at(rect of the widget)`; returns (value, times changed).
    fn click_toggle(start: bool, at: impl Fn(Rect) -> Pos2) -> (bool, u32) {
        let ctx = egui::Context::default();
        let (mut on, mut changes, mut rect) = (start, 0, Rect::NOTHING);
        let mut frame = |events: Vec<egui::Event>, on: &mut bool, rect: &mut Rect| {
            pass(&ctx, events, |ui| {
                let resp = toggle(ui, on, "Start with Windows");
                changes += u32::from(resp.changed());
                *rect = resp.rect;
            });
        };
        frame(vec![], &mut on, &mut rect);
        let p = at(rect);
        frame(vec![egui::Event::PointerMoved(p)], &mut on, &mut rect);
        frame(vec![button(p, true)], &mut on, &mut rect);
        frame(vec![button(p, false)], &mut on, &mut rect);
        frame(vec![], &mut on, &mut rect);
        (on, changes)
    }

    #[test]
    fn a_click_on_the_switch_flips_it_once() {
        assert_eq!(
            click_toggle(false, |r| r.right_center() - vec2(10.0, 0.0)),
            (true, 1)
        );
        assert_eq!(
            click_toggle(true, |r| r.right_center() - vec2(10.0, 0.0)),
            (false, 1)
        );
    }

    #[test]
    fn a_click_on_the_label_flips_it_too() {
        assert_eq!(
            click_toggle(false, |r| r.left_center() + vec2(10.0, 0.0)),
            (true, 1)
        );
    }

    #[test]
    fn the_switch_spans_the_row_with_the_knob_side_on_the_right() {
        let ctx = egui::Context::default();
        let (mut on, mut rect, mut width) = (false, Rect::NOTHING, 0.0);
        pass(&ctx, vec![], |ui| {
            width = ui.available_width();
            rect = toggle(ui, &mut on, "Label").rect;
        });
        assert_eq!(rect.width(), width);
    }

    #[test]
    fn space_flips_a_focused_switch() {
        let ctx = egui::Context::default();
        let (mut on, mut changes) = (false, 0);
        let mut frame = |events: Vec<egui::Event>, focus: bool, on: &mut bool| {
            pass(&ctx, events, |ui| {
                let resp = toggle(ui, on, "Label");
                if focus {
                    resp.request_focus();
                }
                changes += u32::from(resp.changed());
            });
        };
        frame(vec![], true, &mut on);
        frame(vec![key(egui::Key::Space, true)], false, &mut on);
        frame(vec![key(egui::Key::Space, false)], false, &mut on);
        frame(vec![], false, &mut on);
        assert_eq!((on, changes), (true, 1));
    }

    /// Clicks the spot for (`to_edge`, `to_align`) in a picker that starts at `from`.
    fn pick(from: (Edge, Align), to: (Edge, Align)) -> ((Edge, Align), (bool, bool)) {
        let ctx = egui::Context::default();
        let (mut edge, mut align) = from;
        let (mut changed, mut screen) = ((false, false), Rect::NOTHING);
        let mut frame =
            |events: Vec<egui::Event>, edge: &mut Edge, align: &mut Align, screen: &mut Rect| {
                pass(&ctx, events, |ui| {
                    *screen = Rect::from_min_size(ui.cursor().min, SCREEN_SIZE);
                    let (e, a) = place_picker(ui, Lang::En, edge, align);
                    changed = (changed.0 | e, changed.1 | a);
                });
            };
        frame(vec![], &mut edge, &mut align, &mut screen);
        let p = spot_rect(screen, to.0, to.1).center();
        frame(
            vec![egui::Event::PointerMoved(p)],
            &mut edge,
            &mut align,
            &mut screen,
        );
        frame(vec![button(p, true)], &mut edge, &mut align, &mut screen);
        frame(vec![button(p, false)], &mut edge, &mut align, &mut screen);
        ((edge, align), changed)
    }

    #[test]
    fn picking_another_spot_reports_what_changed() {
        let start = (Edge::Right, Align::Center);
        assert_eq!(
            pick(start, (Edge::Left, Align::Top)),
            ((Edge::Left, Align::Top), (true, true))
        );
        assert_eq!(
            pick(start, (Edge::Right, Align::Bottom)),
            ((Edge::Right, Align::Bottom), (false, true))
        );
        assert_eq!(
            pick(start, (Edge::Left, Align::Center)),
            ((Edge::Left, Align::Center), (true, false))
        );
    }

    #[test]
    fn picking_the_current_spot_changes_nothing() {
        let start = (Edge::Right, Align::Center);
        assert_eq!(pick(start, start), (start, (false, false)));
    }

    #[test]
    fn the_six_spots_do_not_overlap_and_stay_inside_the_screen_picture() {
        let screen = Rect::from_min_size(Pos2::ZERO, SCREEN_SIZE);
        let spots: Vec<Rect> = [Edge::Left, Edge::Right]
            .into_iter()
            .flat_map(|e| {
                [Align::Top, Align::Center, Align::Bottom].map(|a| spot_rect(screen, e, a))
            })
            .collect();
        for (i, a) in spots.iter().enumerate() {
            assert!(screen.contains_rect(*a));
            for b in &spots[i + 1..] {
                assert!(!a.intersects(*b));
            }
        }
    }
}
