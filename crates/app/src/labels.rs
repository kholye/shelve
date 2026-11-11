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
}
