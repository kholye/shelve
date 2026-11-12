//! Rows picked with the mouse, as in Explorer: a click picks one, Ctrl adds or takes
//! away, Shift picks the range from the last click.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ShelfItem;

#[derive(Debug, Default)]
pub struct Selection {
    paths: HashSet<PathBuf>,
    /// Where a Shift range starts: the last click without Shift.
    anchor: Option<PathBuf>,
}

impl Selection {
    /// A click on the row of `path`; `items` is the shelf in its order.
    pub fn click(&mut self, items: &[ShelfItem], path: &Path, ctrl: bool, shift: bool) {
        let at = |p: &Path| items.iter().position(|i| i.path == p);
        let range = self.anchor.as_deref().and_then(at).zip(at(path));
        match range {
            Some((a, b)) if shift => {
                let (a, b) = (a.min(b), a.max(b));
                self.paths = items[a..=b].iter().map(|i| i.path.clone()).collect();
            }
            _ => {
                // A plain click on the only selected row takes it away, as Ctrl would.
                let only = self.paths.len() == 1 && self.paths.contains(path);
                if !ctrl && !only {
                    self.paths.clear();
                }
                if !self.paths.remove(path) {
                    self.paths.insert(path.to_owned());
                }
                self.anchor = Some(path.to_owned());
            }
        }
    }

    pub fn clear(&mut self) {
        self.paths.clear();
        self.anchor = None;
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.paths.contains(path)
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Forgets rows no longer among `items`.
    pub fn retain(&mut self, items: &[ShelfItem]) {
        if self.paths.is_empty() && self.anchor.is_none() {
            return;
        }
        let on: HashSet<&Path> = items.iter().map(|i| i.path.as_path()).collect();
        self.paths.retain(|p| on.contains(p.as_path()));
        if self.anchor.as_deref().is_some_and(|a| !on.contains(a)) {
            self.anchor = None;
        }
    }

    /// What a drag that starts on `grabbed` takes: all selected rows in shelf order if it
    /// is one of them, else only it.
    pub fn to_drag(&self, items: &[ShelfItem], grabbed: &ShelfItem) -> Vec<ShelfItem> {
        if !self.contains(&grabbed.path) {
            return vec![grabbed.clone()];
        }
        items
            .iter()
            .filter(|i| self.contains(&i.path))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ItemKind;
    use std::path::PathBuf;

    fn items(n: usize) -> Vec<ShelfItem> {
        (0..n)
            .map(|i| ShelfItem {
                path: PathBuf::from(format!(r"C:\{i}.txt")),
                kind: ItemKind::File,
            })
            .collect()
    }

    fn picked(sel: &Selection, items: &[ShelfItem]) -> Vec<usize> {
        (0..items.len())
            .filter(|&i| sel.contains(&items[i].path))
            .collect()
    }

    #[test]
    fn a_plain_click_selects_only_that_row() {
        let it = items(4);
        let mut sel = Selection::default();
        sel.click(&it, &it[1].path, false, false);
        sel.click(&it, &it[2].path, false, false);
        assert_eq!(picked(&sel, &it), [2]);
    }

    #[test]
    fn a_plain_click_on_the_only_selected_row_deselects_it() {
        let it = items(3);
        let mut sel = Selection::default();
        sel.click(&it, &it[1].path, false, false);
        sel.click(&it, &it[1].path, false, false);
        assert!(sel.is_empty());
    }

    #[test]
    fn a_plain_click_on_one_of_several_selected_keeps_only_it() {
        let it = items(3);
        let mut sel = Selection::default();
        sel.click(&it, &it[0].path, false, false);
        sel.click(&it, &it[1].path, true, false);
        sel.click(&it, &it[1].path, false, false);
        assert_eq!(picked(&sel, &it), [1]);
    }

    #[test]
    fn ctrl_click_adds_and_takes_away() {
        let it = items(4);
        let mut sel = Selection::default();
        sel.click(&it, &it[0].path, false, false);
        sel.click(&it, &it[2].path, true, false);
        assert_eq!(picked(&sel, &it), [0, 2]);
        sel.click(&it, &it[0].path, true, false);
        assert_eq!(picked(&sel, &it), [2]);
    }

    #[test]
    fn shift_click_selects_the_range_from_the_last_click_either_way() {
        let it = items(5);
        let mut sel = Selection::default();
        sel.click(&it, &it[3].path, false, false);
        sel.click(&it, &it[1].path, false, true);
        assert_eq!(picked(&sel, &it), [1, 2, 3]);
        sel.click(&it, &it[4].path, false, true);
        assert_eq!(
            picked(&sel, &it),
            [3, 4],
            "the range starts at the last plain click"
        );
    }

    #[test]
    fn shift_click_with_nothing_clicked_before_selects_one() {
        let it = items(3);
        let mut sel = Selection::default();
        sel.click(&it, &it[2].path, false, true);
        assert_eq!(picked(&sel, &it), [2]);
    }

    #[test]
    fn rows_that_leave_the_shelf_leave_the_selection() {
        let it = items(3);
        let mut sel = Selection::default();
        sel.click(&it, &it[0].path, false, false);
        sel.click(&it, &it[2].path, false, true);
        sel.retain(&it[1..]);
        assert_eq!(picked(&sel, &it), [1, 2]);
        sel.retain(&[]);
        assert!(sel.is_empty());
    }

    #[test]
    fn dragging_a_selected_row_takes_all_selected_in_shelf_order() {
        let it = items(4);
        let mut sel = Selection::default();
        sel.click(&it, &it[3].path, false, false);
        sel.click(&it, &it[0].path, true, false);
        assert_eq!(sel.to_drag(&it, &it[3]), [it[0].clone(), it[3].clone()]);
    }

    #[test]
    fn dragging_an_unselected_row_takes_only_it() {
        let it = items(3);
        let mut sel = Selection::default();
        sel.click(&it, &it[0].path, false, false);
        assert_eq!(sel.to_drag(&it, &it[2]), [it[2].clone()]);
    }

    #[test]
    fn clear_drops_everything() {
        let it = items(2);
        let mut sel = Selection::default();
        sel.click(&it, &it[0].path, false, false);
        sel.clear();
        assert!(sel.is_empty());
        sel.click(&it, &it[1].path, false, true);
        assert_eq!(picked(&sel, &it), [1], "no range from a cleared anchor");
    }
}
