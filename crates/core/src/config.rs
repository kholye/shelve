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
pub fn default_config_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("shelve")
        .join("config.json")
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
    fn default_config_path_ends_with_app_dir() {
        assert!(default_config_path().ends_with(r"shelve\config.json"));
    }
}
