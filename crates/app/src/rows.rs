//! Rows of the shelf as they are drawn: a new one grows in and flashes, a removed
//! one stays while it collapses. Pure state, the clock is passed in.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use shelve_core::ShelfItem;

/// Growing, collapsing and the panel's height change, seconds.
pub const ANIM_S: f32 = 0.18;
/// How long a fresh row stays highlighted, seconds.
pub const FLASH_S: f32 = 0.4;

pub struct Row {
    pub item: ShelfItem,
    /// Height (0..=1) at the moment the current change started.
    from: f32,
    /// When the current change started; None: at rest.
    since: Option<Instant>,
    leaving: bool,
    /// Set for rows added with animations on; drives the flash.
    born: Option<Instant>,
}

#[derive(Default)]
pub struct Rows {
    rows: Vec<Row>,
}

/// Fast start, soft landing, no overshoot. `t` in 0..=1.
pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn secs(from: Instant, now: Instant) -> f32 {
    now.saturating_duration_since(from).as_secs_f32()
}

/// A value (the panel height) that glides to a new target in `ANIM_S`. `animate` off:
/// it is always at its target.
pub struct Glide {
    from: f32,
    to: f32,
    since: Instant,
}

impl Glide {
    pub fn at_rest(value: f32, now: Instant) -> Self {
        Self {
            from: value,
            to: value,
            since: now,
        }
    }

    /// Where it is heading.
    pub fn to(&self) -> f32 {
        self.to
    }

    /// Heads to `to` from wherever the value is now.
    pub fn retarget(&mut self, to: f32, now: Instant, animate: bool) {
        *self = Self {
            from: self.value(now, animate),
            to,
            since: now,
        };
    }

    pub fn settled(&self, now: Instant, animate: bool) -> bool {
        !animate || self.from == self.to || secs(self.since, now) >= ANIM_S
    }

    pub fn value(&self, now: Instant, animate: bool) -> f32 {
        if self.settled(now, animate) {
            return self.to;
        }
        self.from + (self.to - self.from) * ease_out(secs(self.since, now) / ANIM_S)
    }

    /// Room the value needs until it settles: the larger end while gliding, so a
    /// window sized for it is resized once at each end, not per frame.
    pub fn room(&self, now: Instant, animate: bool) -> f32 {
        if self.settled(now, animate) {
            self.to
        } else {
            self.from.max(self.to)
        }
    }
}

impl Rows {
    /// Makes the rows match the shelf: new items are born, missing ones start to
    /// leave, rows that finished leaving are dropped. With `animate` off all of
    /// that happens at once.
    pub fn sync(&mut self, items: &[ShelfItem], now: Instant, animate: bool) {
        // This runs on every frame of a visible shelf. Everything below is looked up
        // by path in a map, so the cost grows with the item count, not its square.
        let heights: Vec<f32> = self.rows.iter().map(|r| self.height(r, now)).collect();
        let on_shelf: HashSet<&Path> = items.iter().map(|i| i.path.as_path()).collect();
        // Rows gone from the shelf, grouped by the surviving row they followed
        // (None: they were above every survivor).
        let mut leaving: HashMap<Option<PathBuf>, Vec<Row>> = HashMap::new();
        let mut kept: HashMap<PathBuf, Row> = HashMap::with_capacity(items.len());
        let mut anchor = None;
        for (mut row, h) in std::mem::take(&mut self.rows).into_iter().zip(heights) {
            if on_shelf.contains(row.item.path.as_path()) {
                if row.leaving {
                    // Put back mid-collapse: grow again from where it is.
                    row.leaving = false;
                    (row.from, row.since) = (h, animate.then_some(now));
                }
                anchor = Some(row.item.path.clone());
                kept.insert(row.item.path.clone(), row);
                continue;
            }
            let done = row.leaving && row.since.is_some_and(|s| secs(s, now) >= ANIM_S);
            if !animate || done {
                continue;
            }
            if !row.leaving {
                row.leaving = true;
                (row.from, row.since) = (h, Some(now));
            }
            leaving.entry(anchor.clone()).or_default().push(row);
        }

        let mut next = Vec::with_capacity(items.len() + 1);
        next.extend(leaving.remove(&None).unwrap_or_default());
        for item in items {
            let row = kept.remove(&item.path).unwrap_or_else(|| Row {
                item: item.clone(),
                from: 0.0,
                since: animate.then_some(now),
                leaving: false,
                born: animate.then_some(now),
            });
            next.push(row);
            if !leaving.is_empty() {
                next.extend(leaving.remove(&Some(item.path.clone())).unwrap_or_default());
            }
        }
        self.rows = next;
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Share of the full row height, 0..=1.
    pub fn height(&self, row: &Row, now: Instant) -> f32 {
        let target = if row.leaving { 0.0 } else { 1.0 };
        match row.since {
            Some(since) => row.from + (target - row.from) * ease_out(secs(since, now) / ANIM_S),
            None => target,
        }
    }

    /// Highlight of a fresh row: 1 right after it was added, 0 after `FLASH_S`.
    pub fn flash(&self, row: &Row, now: Instant) -> f32 {
        row.born.map_or(0.0, |born| {
            (1.0 - secs(born, now) / FLASH_S).clamp(0.0, 1.0)
        })
    }

    pub fn is_leaving(&self, row: &Row) -> bool {
        row.leaving
    }

    /// Something still moves or fades, so the shelf must keep repainting.
    pub fn animating(&self, now: Instant) -> bool {
        self.rows.iter().any(|r| {
            r.leaving
                || r.since.is_some_and(|s| secs(s, now) < ANIM_S)
                || r.born.is_some_and(|b| secs(b, now) < FLASH_S)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shelve_core::ItemKind;
    use std::path::PathBuf;
    use std::time::Duration;

    fn item(name: &str) -> ShelfItem {
        ShelfItem {
            path: PathBuf::from(format!(r"C:\shelve-нет\{name}")),
            kind: ItemKind::File,
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_glide_at_rest_stays_at_its_value() {
        let t0 = Instant::now();
        let g = Glide::at_rest(132.0, t0);
        assert!(g.settled(t0, true));
        assert_eq!(g.value(t0 + ms(50), true), 132.0);
        assert_eq!(g.room(t0, true), 132.0);
    }

    #[test]
    fn a_glide_eases_to_the_new_value_and_settles() {
        let t0 = Instant::now();
        let mut g = Glide::at_rest(84.0, t0);
        g.retarget(172.0, t0, true);
        assert_eq!(g.value(t0, true), 84.0);
        let half = g.value(t0 + ms(90), true);
        assert!(half > 128.0 && half < 172.0, "got {half}");
        assert!(!g.settled(t0 + ms(90), true));
        assert_eq!(g.value(t0 + ms(180), true), 172.0);
        assert!(g.settled(t0 + ms(180), true));
    }

    #[test]
    fn a_gliding_value_has_room_for_the_taller_end_until_it_settles() {
        // The window is resized once at each end of the glide, not per frame.
        let t0 = Instant::now();
        let mut g = Glide::at_rest(172.0, t0);
        g.retarget(84.0, t0, true);
        assert_eq!(g.room(t0 + ms(90), true), 172.0);
        assert_eq!(g.room(t0 + ms(180), true), 84.0);
        g.retarget(172.0, t0 + ms(1000), true);
        assert_eq!(g.room(t0 + ms(1001), true), 172.0);
    }

    #[test]
    fn a_glide_retargeted_midway_starts_from_where_it_is() {
        let t0 = Instant::now();
        let mut g = Glide::at_rest(84.0, t0);
        g.retarget(172.0, t0, true);
        let before = g.value(t0 + ms(60), true);
        g.retarget(84.0, t0 + ms(60), true);
        let after = g.value(t0 + ms(60), true);
        assert!((after - before).abs() < 0.01, "{before} -> {after}");
        assert_eq!(g.to(), 84.0);
    }

    #[test]
    fn a_glide_without_animations_jumps() {
        let t0 = Instant::now();
        let mut g = Glide::at_rest(84.0, t0);
        g.retarget(172.0, t0, false);
        assert!(g.settled(t0, false));
        assert_eq!(g.value(t0, false), 172.0);
        assert_eq!(g.room(t0, false), 172.0);
    }

    #[test]
    fn a_new_row_grows_from_nothing_to_full_height() {
        let (t0, a) = (Instant::now(), item("a"));
        let mut rows = Rows::default();
        rows.sync(&[a], t0, true);
        assert_eq!(rows.height(&rows.rows()[0], t0), 0.0);
        let half = rows.height(&rows.rows()[0], t0 + ms(90));
        assert!(
            half > 0.5 && half < 1.0,
            "ease-out: past half way at half time, got {half}"
        );
        assert_eq!(rows.height(&rows.rows()[0], t0 + ms(180)), 1.0);
        assert_eq!(rows.height(&rows.rows()[0], t0 + ms(5000)), 1.0);
    }

    #[test]
    fn animating_lasts_through_growth_and_flash_then_stops() {
        let (t0, a) = (Instant::now(), item("a"));
        let mut rows = Rows::default();
        assert!(!rows.animating(t0));
        rows.sync(&[a], t0, true);
        assert!(rows.animating(t0 + ms(90)));
        assert!(rows.animating(t0 + ms(300)), "still flashing");
        assert!(!rows.animating(t0 + ms(500)));
    }

    #[test]
    fn a_new_row_flashes_and_fades() {
        let (t0, a) = (Instant::now(), item("a"));
        let mut rows = Rows::default();
        rows.sync(&[a], t0, true);
        assert_eq!(rows.flash(&rows.rows()[0], t0), 1.0);
        let mid = rows.flash(&rows.rows()[0], t0 + ms(200));
        assert!((mid - 0.5).abs() < 0.01, "got {mid}");
        assert_eq!(rows.flash(&rows.rows()[0], t0 + ms(400)), 0.0);
    }

    #[test]
    fn a_removed_row_stays_while_it_collapses_then_goes() {
        let (t0, a) = (Instant::now(), item("a"));
        let mut rows = Rows::default();
        rows.sync(&[a], t0, true);
        rows.sync(&[], t0 + ms(1000), true);
        assert_eq!(rows.rows().len(), 1);
        assert!(rows.is_leaving(&rows.rows()[0]));
        assert!(rows.animating(t0 + ms(1090)));
        let h = rows.height(&rows.rows()[0], t0 + ms(1090));
        assert!(h > 0.0 && h < 0.5, "got {h}");
        rows.sync(&[], t0 + ms(1200), true);
        assert!(rows.rows().is_empty());
        assert!(!rows.animating(t0 + ms(1200)));
    }

    #[test]
    fn without_animations_everything_is_instant() {
        let (t0, a) = (Instant::now(), item("a"));
        let mut rows = Rows::default();
        rows.sync(&[a], t0, false);
        assert_eq!(rows.height(&rows.rows()[0], t0), 1.0);
        assert_eq!(rows.flash(&rows.rows()[0], t0), 0.0);
        assert!(!rows.animating(t0));
        rows.sync(&[], t0, false);
        assert!(rows.rows().is_empty());
    }

    #[test]
    fn animations_switched_off_mid_collapse_drop_the_row_at_once() {
        // No row may stay stuck half removed.
        let (t0, a) = (Instant::now(), item("a"));
        let mut rows = Rows::default();
        rows.sync(&[a], t0, true);
        rows.sync(&[], t0 + ms(1000), true);
        rows.sync(&[], t0 + ms(1010), false);
        assert!(rows.rows().is_empty());
    }

    #[test]
    fn a_file_put_back_while_its_row_collapses_revives_the_row() {
        let (t0, a) = (Instant::now(), item("a"));
        let mut rows = Rows::default();
        rows.sync(std::slice::from_ref(&a), t0, true);
        rows.sync(&[], t0 + ms(1000), true);
        let before = rows.height(&rows.rows()[0], t0 + ms(1050));
        rows.sync(&[a], t0 + ms(1050), true);
        assert_eq!(rows.rows().len(), 1);
        assert!(!rows.is_leaving(&rows.rows()[0]));
        // It grows back from where it was, without a jump.
        let after = rows.height(&rows.rows()[0], t0 + ms(1050));
        assert!((after - before).abs() < 0.01, "{before} -> {after}");
        assert_eq!(rows.height(&rows.rows()[0], t0 + ms(1300)), 1.0);
    }

    #[test]
    fn a_row_removed_while_still_growing_collapses_from_where_it_was() {
        let (t0, a) = (Instant::now(), item("a"));
        let mut rows = Rows::default();
        rows.sync(&[a], t0, true);
        let before = rows.height(&rows.rows()[0], t0 + ms(60));
        rows.sync(&[], t0 + ms(60), true);
        let after = rows.height(&rows.rows()[0], t0 + ms(60));
        assert!((after - before).abs() < 0.01, "{before} -> {after}");
    }

    #[test]
    fn rows_keep_the_order_of_the_shelf() {
        let (t0, a, b) = (Instant::now(), item("a"), item("b"));
        let mut rows = Rows::default();
        rows.sync(&[a.clone(), b.clone()], t0, false);
        assert_eq!(rows.rows()[0].item.path, a.path);
        assert_eq!(rows.rows()[1].item.path, b.path);
    }

    #[test]
    fn a_collapsing_row_keeps_its_place_between_its_neighbours() {
        let (t0, a, b, c) = (Instant::now(), item("a"), item("b"), item("c"));
        let mut rows = Rows::default();
        rows.sync(&[a.clone(), b.clone(), c.clone()], t0, false);
        rows.sync(&[a.clone(), c.clone()], t0 + ms(10), true);
        let order: Vec<_> = rows.rows().iter().map(|r| r.item.path.clone()).collect();
        assert_eq!(order, [a.path, b.path, c.path]);
        assert!(rows.is_leaving(&rows.rows()[1]));
    }

    #[test]
    fn clearing_many_rows_collapses_them_all_and_ends_empty() {
        let t0 = Instant::now();
        let items: Vec<_> = (0..300).map(|i| item(&i.to_string())).collect();
        let mut rows = Rows::default();
        rows.sync(&items, t0, false);
        rows.sync(&[], t0 + ms(10), true);
        assert_eq!(rows.rows().len(), 300);
        rows.sync(&[], t0 + ms(400), true);
        assert!(rows.rows().is_empty());
    }
    #[test]
    fn syncing_thousands_of_unchanged_rows_is_fast() {
        // It runs on every frame, so it must not grow with the square of the count:
        // at 1000 files that was 100 ms per frame.
        let t0 = Instant::now();
        let items: Vec<_> = (0..3000).map(|i| item(&i.to_string())).collect();
        let mut rows = Rows::default();
        rows.sync(&items, t0, false);
        let clock = Instant::now();
        for _ in 0..3 {
            rows.sync(&items, t0, false);
        }
        let per_sync = clock.elapsed() / 3;
        assert!(per_sync < Duration::from_millis(50), "took {per_sync:?}");
        assert_eq!(rows.rows().len(), 3000);
    }

    #[test]
    fn removing_every_other_row_of_many_keeps_order_and_is_fast() {
        let t0 = Instant::now();
        let items: Vec<_> = (0..3000).map(|i| item(&i.to_string())).collect();
        let kept: Vec<_> = items.iter().step_by(2).cloned().collect();
        let mut rows = Rows::default();
        rows.sync(&items, t0, false);
        let clock = Instant::now();
        rows.sync(&kept, t0 + ms(10), true);
        assert!(
            clock.elapsed() < Duration::from_millis(150),
            "took {:?}",
            clock.elapsed()
        );
        let order: Vec<_> = rows.rows().iter().map(|r| r.item.path.clone()).collect();
        let expected: Vec<_> = items.iter().map(|i| i.path.clone()).collect();
        assert_eq!(order, expected);
        assert!(rows.is_leaving(&rows.rows()[1]) && !rows.is_leaving(&rows.rows()[2]));
    }
}
