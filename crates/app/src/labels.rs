//! Short texts on the shelf that are computed, not fixed.

use std::path::{Component, Path, Prefix};

/// Where the file comes from, for the second line of a row: the folder's name,
/// or the drive / network share when the file lies in its root. Never empty.
pub fn folder_label(path: &Path) -> String {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(path);
    match dir.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => root_label(dir),
    }
}

/// A name too long for `fits` loses its middle, so the end with the extension
/// (and four letters before it) stays in sight. A name without one is cut at the end.
/// When even "…" plus the end does not fit, that is returned and the caller cuts it.
pub fn cut_middle(name: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(name) {
        return name.to_owned();
    }
    let chars: Vec<char> = name.chars().collect();
    let dot = chars.iter().rposition(|&c| c == '.');
    let is_ext = |d: usize| {
        let ext = &chars[d + 1..];
        d > 0 && (1..=8).contains(&ext.len()) && !ext.iter().any(|c| c.is_whitespace())
    };
    let tail_from = match dot {
        Some(d) if is_ext(d) => d.saturating_sub(4),
        _ => chars.len(),
    };
    let tail: String = chars[tail_from..].iter().collect();
    let joined = |k: usize| format!("{}…{tail}", chars[..k].iter().collect::<String>());
    // The longest head that still fits; 0 if none does.
    let (mut lo, mut hi) = (0, tail_from);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(&joined(mid)) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    joined(lo)
}

/// The whole folder the file lies in, for the second line of the row under the mouse.
pub fn folder_path(path: &Path) -> String {
    match path.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(dir) => dir.display().to_string(),
        None => folder_label(path),
    }
}

/// A path too long for `fits` loses whole folders in its middle: the drive (or the
/// network server and share) and the last folder always stay. When even that does not
/// fit, it is returned and the caller cuts it.
pub fn cut_path(path: &str, fits: impl Fn(&str) -> bool) -> String {
    let parts: Vec<&str> = path.split('\\').collect();
    let n = parts.len();
    let root = if path.starts_with(r"\\") { 4 } else { 1 };
    if fits(path) || n <= root + 1 {
        return path.to_owned();
    }
    let joined =
        |h: usize, t: usize| format!(r"{}\…\{}", parts[..h].join(r"\"), parts[n - t..].join(r"\"));
    // Grow the end and the start by turns, while a folder is still left out.
    let (mut h, mut t) = (root, 1);
    let mut grew = true;
    while grew {
        grew = false;
        if h + t + 1 < n && fits(&joined(h, t + 1)) {
            t += 1;
            grew = true;
        }
        if h + t + 1 < n && fits(&joined(h + 1, t)) {
            h += 1;
            grew = true;
        }
    }
    joined(h, t)
}

fn root_label(root: &Path) -> String {
    if let Some(Component::Prefix(prefix)) = root.components().next() {
        if let Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) = prefix.kind() {
            return format!(
                r"\\{}\{}",
                server.to_string_lossy(),
                share.to_string_lossy()
            );
        }
    }
    root.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_is_the_name_of_the_parent_directory() {
        assert_eq!(folder_label(Path::new(r"C:\Users\a\Папка\x.txt")), "Папка");
    }

    #[test]
    fn file_in_a_drive_root_shows_the_drive() {
        assert_eq!(folder_label(Path::new(r"D:\x.txt")), r"D:\");
    }

    #[test]
    fn file_in_a_network_share_root_shows_the_share() {
        assert_eq!(
            folder_label(Path::new(r"\\server\share\x.txt")),
            r"\\server\share"
        );
    }

    #[test]
    fn file_deeper_in_a_network_share_shows_its_folder() {
        assert_eq!(folder_label(Path::new(r"\\server\share\dir\x.txt")), "dir");
    }

    #[test]
    fn a_drive_or_share_put_on_the_shelf_itself_is_never_blank() {
        assert_eq!(folder_label(Path::new(r"D:\")), r"D:\");
        assert_eq!(
            folder_label(Path::new(r"\\server\share\")),
            r"\\server\share"
        );
        assert!(!folder_label(Path::new("x.txt")).is_empty());
    }

    fn up_to(n: usize) -> impl Fn(&str) -> bool {
        move |s: &str| s.chars().count() <= n
    }

    #[test]
    fn a_name_that_fits_is_left_alone() {
        assert_eq!(cut_middle("Договор.pdf", up_to(20)), "Договор.pdf");
    }

    #[test]
    fn a_long_name_loses_its_middle_and_keeps_the_extension() {
        let cut = cut_middle("Договор аренды (подписанный).pdf", up_to(20));
        assert_eq!(cut, "Договор аре…ный).pdf");
    }

    #[test]
    fn a_name_without_extension_is_cut_at_the_end() {
        assert_eq!(cut_middle("Длинная папка проекта", up_to(10)), "Длинная п…");
    }

    #[test]
    fn a_dot_that_is_not_an_extension_is_ignored() {
        // A hidden name and a sentence with a dot inside are cut at the end.
        assert_eq!(cut_middle(".gitignore-backup", up_to(8)), ".gitign…");
        assert_eq!(
            cut_middle("Отчёт v1.2 итоговый вариант", up_to(10)),
            "Отчёт v1.…"
        );
    }

    #[test]
    fn too_narrow_for_the_extension_keeps_only_the_end() {
        assert_eq!(cut_middle("Договор аренды.pdf", up_to(4)), "…енды.pdf");
    }

    #[test]
    fn folder_path_is_the_whole_parent_directory() {
        assert_eq!(
            folder_path(Path::new(r"C:\Users\a\Папка\x.txt")),
            r"C:\Users\a\Папка"
        );
        assert_eq!(folder_path(Path::new(r"D:\x.txt")), r"D:\");
        assert!(!folder_path(Path::new(r"D:\")).is_empty());
    }

    #[test]
    fn a_path_that_fits_is_left_alone() {
        assert_eq!(cut_path(r"C:\a\b", up_to(20)), r"C:\a\b");
    }

    #[test]
    fn a_long_path_loses_whole_folders_in_the_middle() {
        let path = r"C:\Users\admin\sandbox\design\Пять";
        assert_eq!(cut_path(path, up_to(28)), r"C:\Users\admin\…\design\Пять");
        assert_eq!(cut_path(path, up_to(22)), r"C:\Users\…\design\Пять");
        assert_eq!(cut_path(path, up_to(12)), r"C:\…\Пять");
    }

    #[test]
    fn a_network_path_keeps_its_server_and_share() {
        let path = r"\\server\share\one\two\three\Пять";
        assert_eq!(cut_path(path, up_to(22)), r"\\server\share\…\Пять");
    }

    #[test]
    fn too_narrow_for_any_folder_keeps_the_root_and_the_last_one() {
        assert_eq!(cut_path(r"C:\Users\admin\Пять", up_to(3)), r"C:\…\Пять");
    }
}
