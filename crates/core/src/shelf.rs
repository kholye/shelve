use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    File,
    Dir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Present,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShelfItem {
    pub path: PathBuf,
    pub kind: ItemKind,
}

#[derive(Debug, Default)]
pub struct Shelf {
    items: Vec<ShelfItem>,
}

impl Shelf {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns false if the path is already on the shelf.
    pub fn add(&mut self, path: PathBuf) -> bool {
        if self.items.iter().any(|i| same_path(&i.path, &path)) {
            return false;
        }
        let kind = if path.is_dir() {
            ItemKind::Dir
        } else {
            ItemKind::File
        };
        self.items.push(ShelfItem { path, kind });
        true
    }

    pub fn remove(&mut self, path: &Path) -> bool {
        let before = self.items.len();
        self.items.retain(|i| !same_path(&i.path, path));
        self.items.len() != before
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn items(&self) -> &[ShelfItem] {
        &self.items
    }
}

pub fn availability(item: &ShelfItem) -> Availability {
    if item.path.exists() {
        Availability::Present
    } else {
        Availability::Missing
    }
}

// Windows paths are case-insensitive. Note: Unicode lowercase, not NTFS upcase table; fine for dedupe.
#[cfg(windows)]
fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
}

#[cfg(not(windows))]
fn same_path(a: &Path, b: &Path) -> bool {
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_returns_false_for_duplicate() {
        let mut shelf = Shelf::new();
        assert!(shelf.add(PathBuf::from(r"C:\a.txt")));
        assert!(!shelf.add(PathBuf::from(r"C:\a.txt")));
        assert_eq!(shelf.items().len(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn add_dedupes_case_insensitively_on_windows() {
        let mut shelf = Shelf::new();
        assert!(shelf.add(PathBuf::from(r"C:\Папка\A.txt")));
        assert!(!shelf.add(PathBuf::from(r"c:\папка\a.TXT")));
        assert_eq!(shelf.items().len(), 1);
    }

    #[test]
    fn add_keeps_cyrillic_and_unc_paths_verbatim() {
        let mut shelf = Shelf::new();
        let paths = [
            PathBuf::from(r"C:\Папка с пробелом\файл.txt"),
            PathBuf::from(r"\\srv\share\док.pdf"),
        ];
        for p in &paths {
            assert!(shelf.add(p.clone()));
        }
        for (i, p) in paths.iter().enumerate() {
            assert_eq!(&shelf.items()[i].path, p);
        }
    }

    #[test]
    fn remove_and_clear() {
        let mut shelf = Shelf::new();
        shelf.add(PathBuf::from(r"C:\a.txt"));
        shelf.add(PathBuf::from(r"C:\b.txt"));
        assert!(shelf.remove(Path::new(r"C:\a.txt")));
        assert!(!shelf.remove(Path::new(r"C:\missing.txt")));
        assert_eq!(shelf.items().len(), 1);
        shelf.clear();
        assert!(shelf.items().is_empty());
    }

    #[test]
    fn kind_is_dir_for_directory_and_file_for_missing_path() {
        let mut shelf = Shelf::new();
        let dir = std::env::temp_dir();
        shelf.add(dir.clone());
        shelf.add(dir.join("shelve-no-such-file.txt"));
        assert_eq!(shelf.items()[0].kind, ItemKind::Dir);
        assert_eq!(shelf.items()[1].kind, ItemKind::File);
    }

    #[test]
    fn availability_missing_after_delete() {
        let path = std::env::temp_dir().join(format!("shelve-avail-{}.txt", std::process::id()));
        std::fs::write(&path, b"x").unwrap();
        let mut shelf = Shelf::new();
        shelf.add(path.clone());
        assert_eq!(availability(&shelf.items()[0]), Availability::Present);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(availability(&shelf.items()[0]), Availability::Missing);
    }
}
