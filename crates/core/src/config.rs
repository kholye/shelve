use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

use crate::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub edge: Edge,
    pub align: Align,
    pub hotkey: String,
    pub autostart: bool,
    pub auto_show: bool,
    pub edge_zone_px: u32,
    pub hide_delay_ms: u32,
    pub lang: Lang,
    /// Ask GitHub once a day whether a newer version is out.
    pub check_updates: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            edge: Edge::Right,
            align: Align::Center,
            hotkey: "Ctrl+Alt+D".into(),
            autostart: false,
            auto_show: true,
            edge_zone_px: 4,
            hide_delay_ms: 1500,
            lang: Lang::En,
            check_updates: true,
        }
    }
}

impl Config {
    /// Never fails: a missing, empty or corrupt file yields the defaults.
    pub fn load(path: &Path) -> Config {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(self)?)
    }
}

/// `%APPDATA%\shelve\config.json`.
fn appdata_config(appdata: Option<&Path>) -> PathBuf {
    appdata
        .map(Path::to_path_buf)
        .unwrap_or_default()
        .join("shelve")
        .join("config.json")
}

/// The running exe's folder and %APPDATA%.
fn here() -> (PathBuf, Option<PathBuf>) {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    (exe_dir, std::env::var_os("APPDATA").map(PathBuf::from))
}

/// Lying next to the exe, this file holds the settings and makes Shelve portable:
/// nothing is written to %APPDATA% or the registry.
pub const PORTABLE_FILE: &str = "shelve.json";

/// Where the settings are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPlace {
    pub path: PathBuf,
    /// Next to the exe.
    pub portable: bool,
}

impl ConfigPlace {
    /// Next to the exe if `shelve.json` is there, else in %APPDATA%.
    pub fn find(exe_dir: &Path, appdata: Option<&Path>) -> Self {
        let portable = exe_dir.join(PORTABLE_FILE);
        if portable.is_file() {
            Self {
                path: portable,
                portable: true,
            }
        } else {
            Self {
                path: appdata_config(appdata),
                portable: false,
            }
        }
    }

    /// For the running exe.
    pub fn current() -> Self {
        let (exe_dir, appdata) = here();
        Self::find(&exe_dir, appdata.as_deref())
    }

    /// `switch` for the running exe.
    pub fn switch_current(cfg: &Config, portable: bool) -> io::Result<Self> {
        let (exe_dir, appdata) = here();
        Self::switch(cfg, &exe_dir, appdata.as_deref(), portable)
    }

    /// Writes `cfg` to the new place, then removes the old file (and the %APPDATA%
    /// folder if nothing else is in it). On failure the old place stays as it was.
    pub fn switch(
        cfg: &Config,
        exe_dir: &Path,
        appdata: Option<&Path>,
        portable: bool,
    ) -> io::Result<Self> {
        let (new, old) = (exe_dir.join(PORTABLE_FILE), appdata_config(appdata));
        let (new, old) = if portable { (new, old) } else { (old, new) };
        cfg.save(&new)?;
        match std::fs::remove_file(&old) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => println!("old settings kept: {e}"),
            _ => {}
        }
        if portable {
            if let Some(dir) = old.parent() {
                // Fails while anything else is inside, which is meant.
                let _ = std::fs::remove_dir(dir);
            }
        }
        Ok(Self::find(exe_dir, appdata))
    }

    /// Whether Windows should start Shelve: never when portable, since that would write
    /// to the registry, and a flash drive may come back under another letter.
    pub fn autostart(&self, cfg: &Config) -> bool {
        cfg.autostart && !self.portable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("shelve-cfg-{}-{}", std::process::id(), name))
            .join("config.json")
    }

    #[test]
    fn default_values_match_spec() {
        let c = Config::default();
        assert_eq!(c.edge, Edge::Right);
        assert_eq!(c.align, Align::Center);
        assert_eq!(c.hotkey, "Ctrl+Alt+D");
        assert!(c.check_updates);
    }

    #[test]
    fn config_from_before_the_update_check_turns_it_on() {
        let c = load_with_content("old", r#"{"edge":"left","auto_show":false}"#);
        assert_eq!(c.edge, Edge::Left);
        assert!(c.check_updates);
    }

    #[test]
    fn load_missing_file_returns_default() {
        assert_eq!(Config::load(&temp_path("missing")), Config::default());
    }

    fn load_with_content(name: &str, content: &str) -> Config {
        let path = temp_path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        let c = Config::load(&path);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        c
    }

    #[test]
    fn load_corrupt_json_returns_default() {
        assert_eq!(load_with_content("corrupt", "{not json"), Config::default());
    }

    #[test]
    fn load_empty_file_returns_default() {
        assert_eq!(load_with_content("empty", ""), Config::default());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let path = temp_path("roundtrip");
        let c = Config {
            edge: Edge::Left,
            ..Config::default()
        };
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path), c);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn language_is_english_until_chosen_and_then_remembered() {
        // A config written before the language setting existed has no "lang".
        let old = load_with_content("no-lang", r#"{"edge":"left"}"#);
        assert_eq!(old.lang, Lang::En);
        let path = temp_path("lang");
        let c = Config {
            lang: Lang::Ru,
            ..Config::default()
        };
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path).lang, Lang::Ru);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn load_ignores_unknown_and_fills_missing_fields() {
        let c = load_with_content("partial", r#"{"edge":"left","future_option":1}"#);
        assert_eq!(
            c,
            Config {
                edge: Edge::Left,
                ..Config::default()
            }
        );
    }

    #[test]
    fn a_config_with_the_removed_dwell_setting_still_loads() {
        let c = load_with_content("dwell", r#"{"edge":"left","edge_dwell_ms":250}"#);
        assert_eq!(c.edge, Edge::Left);
    }

    #[test]
    fn appdata_config_ends_with_app_dir() {
        let path = appdata_config(Some(Path::new(r"C:\Users\x\AppData\Roaming")));
        assert!(path.ends_with(r"Roaming\shelve\config.json"));
    }

    /// A folder for the exe and one standing in for %APPDATA%, both empty.
    fn dirs(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("shelve-place-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (exe, appdata) = (root.join("usb"), root.join("appdata"));
        std::fs::create_dir_all(&exe).unwrap();
        std::fs::create_dir_all(&appdata).unwrap();
        (root, exe, appdata)
    }

    #[test]
    fn without_the_file_next_to_the_exe_settings_stay_in_appdata() {
        let (root, exe, appdata) = dirs("installed");
        let place = ConfigPlace::find(&exe, Some(&appdata));
        assert!(!place.portable);
        assert_eq!(place.path, appdata.join("shelve").join("config.json"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_file_next_to_the_exe_makes_it_portable() {
        let (root, exe, appdata) = dirs("portable");
        std::fs::write(exe.join(PORTABLE_FILE), "").unwrap();
        let place = ConfigPlace::find(&exe, Some(&appdata));
        assert!(place.portable);
        assert_eq!(place.path, exe.join("shelve.json"));
        // An empty file is a valid way to switch it on: the defaults apply.
        assert_eq!(Config::load(&place.path), Config::default());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn going_portable_moves_the_settings_and_leaves_no_folder_in_appdata() {
        let (root, exe, appdata) = dirs("to-portable");
        let cfg = Config {
            edge: Edge::Left,
            ..Config::default()
        };
        cfg.save(&ConfigPlace::find(&exe, Some(&appdata)).path)
            .unwrap();
        let place = ConfigPlace::switch(&cfg, &exe, Some(&appdata), true).unwrap();
        assert_eq!(place, ConfigPlace::find(&exe, Some(&appdata)));
        assert!(place.portable);
        assert_eq!(Config::load(&place.path).edge, Edge::Left);
        assert!(!appdata.join("shelve").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn going_portable_keeps_other_files_in_the_old_folder() {
        let (root, exe, appdata) = dirs("keep-others");
        let old = appdata.join("shelve");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("config.json"), "{}").unwrap();
        std::fs::write(old.join("not-ours.txt"), "x").unwrap();
        ConfigPlace::switch(&Config::default(), &exe, Some(&appdata), true).unwrap();
        assert!(!old.join("config.json").exists());
        assert!(old.join("not-ours.txt").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn leaving_portable_moves_the_settings_back_to_appdata() {
        let (root, exe, appdata) = dirs("to-installed");
        let cfg = Config {
            lang: Lang::Ru,
            ..Config::default()
        };
        ConfigPlace::switch(&cfg, &exe, Some(&appdata), true).unwrap();
        let place = ConfigPlace::switch(&cfg, &exe, Some(&appdata), false).unwrap();
        assert!(!place.portable);
        assert!(!exe.join(PORTABLE_FILE).exists());
        assert_eq!(Config::load(&place.path).lang, Lang::Ru);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_folder_that_cannot_be_written_keeps_the_old_place() {
        let (root, _, appdata) = dirs("read-only");
        // A file where the folder should be: nothing can be written inside it.
        let blocked = root.join("blocked");
        std::fs::write(&blocked, "x").unwrap();
        Config::default()
            .save(&appdata_config(Some(&appdata)))
            .unwrap();
        assert!(ConfigPlace::switch(&Config::default(), &blocked, Some(&appdata), true).is_err());
        assert!(!ConfigPlace::find(&blocked, Some(&appdata)).portable);
        assert!(appdata_config(Some(&appdata)).exists(), "old settings kept");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_never_starts_with_windows() {
        let on = Config {
            autostart: true,
            ..Config::default()
        };
        let place = |portable| ConfigPlace {
            path: PathBuf::new(),
            portable,
        };
        assert!(place(false).autostart(&on));
        assert!(!place(true).autostart(&on));
        assert!(!place(false).autostart(&Config::default()));
    }
}
