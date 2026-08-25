//! The user configuration file.
//!
//! JSON, GUI-managed later, hand-editable always. The escape-hatch rules
//! (inherited from textchum): a missing file is simply the defaults; a
//! broken file is *never* clobbered — defaults apply, and the load warning
//! is surfaced so the shell can tell the user; unknown keys are preserved
//! by ignoring them on read (nothing writes the file yet).

use std::collections::BTreeMap;
use std::path::Path;

/// Loaded configuration. Phase 1 carries only key bindings; more settings
/// join as their features land.
pub struct Config {
    /// Action name → key spec overrides (e.g. `"next-hunk": "cmd+alt+down"`).
    /// An empty string unbinds the action's default.
    keys: BTreeMap<String, String>,
    load_warning: Option<String>,
}

impl Config {
    /// Loads `path`. Never fails: problems become defaults plus a warning.
    pub fn load(path: &Path) -> Self {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Self::default();
            }
            Err(error) => {
                return Self::warned(format!("could not read {}: {error}", path.display()));
            }
        };
        Self::from_json(&text)
    }

    /// Parses configuration JSON. Never fails: problems become defaults
    /// plus a warning.
    pub fn from_json(text: &str) -> Self {
        let value: serde_json::Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(error) => {
                return Self::warned(format!("config is not valid JSON: {error}"));
            }
        };
        let Some(object) = value.as_object() else {
            return Self::warned("config must be a JSON object".to_string());
        };
        let mut config = Self::default();
        if let Some(keys) = object.get("keys") {
            match keys.as_object() {
                Some(map) => {
                    for (action, spec) in map {
                        match spec.as_str() {
                            Some(spec) => {
                                config.keys.insert(action.clone(), spec.to_string());
                            }
                            None => {
                                config.load_warning = Some(format!(
                                    "keys.{action} must be a string; ignored"
                                ));
                            }
                        }
                    }
                }
                None => {
                    config.load_warning =
                        Some("keys must be an object of action → key spec".to_string());
                }
            }
        }
        config
    }

    fn warned(message: String) -> Self {
        Self {
            keys: BTreeMap::new(),
            load_warning: Some(message),
        }
    }

    /// The problem found while loading, if any. Defaults are in effect
    /// when this is set; the file on disk was left untouched.
    pub fn load_warning(&self) -> Option<&str> {
        self.load_warning.as_deref()
    }

    /// Key-binding overrides as a JSON object string.
    pub fn keys_json(&self) -> String {
        serde_json::to_string(&self.keys).unwrap_or_else(|_| "{}".to_string())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keys: BTreeMap::new(),
            load_warning: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_defaults() {
        let config = Config::load(Path::new("/nonexistent/prchum-config.json"));
        assert!(config.load_warning().is_none());
        assert_eq!(config.keys_json(), "{}");
    }

    #[test]
    fn keys_override_round_trip() {
        let config = Config::from_json(
            r#"{"keys": {"next-hunk": "cmd+alt+down", "toggle-wrap": ""}}"#,
        );
        assert!(config.load_warning().is_none());
        assert_eq!(
            config.keys_json(),
            r#"{"next-hunk":"cmd+alt+down","toggle-wrap":""}"#
        );
    }

    #[test]
    fn broken_json_warns_and_defaults() {
        let config = Config::from_json("{ broken");
        assert!(config.load_warning().is_some());
        assert_eq!(config.keys_json(), "{}");
    }

    #[test]
    fn wrong_shapes_warn_but_keep_going() {
        let config = Config::from_json(r#"{"keys": {"next-hunk": 3}}"#);
        assert!(config.load_warning().unwrap().contains("next-hunk"));
        assert_eq!(config.keys_json(), "{}");

        let config = Config::from_json(r#"{"keys": []}"#);
        assert!(config.load_warning().is_some());
    }

    #[test]
    fn unknown_top_level_keys_are_tolerated() {
        let config = Config::from_json(r#"{"future_setting": true, "keys": {"open": "cmd+o"}}"#);
        assert!(config.load_warning().is_none());
        assert_eq!(config.keys_json(), r#"{"open":"cmd+o"}"#);
    }
}
