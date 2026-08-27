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
    /// Host → forge kind (`github` | `gitlab` | `forgejo`) for self-hosted
    /// instances heuristics can't classify (e.g. `git.example.com`).
    forges: BTreeMap<String, String>,
    /// Command template for Forgejo API calls, with `{host}`, `{method}`,
    /// and `{path}` placeholders; the JSON body arrives on stdin. Empty
    /// means the built-in default (the `fj` CLI).
    forgejo_api_command: String,
    /// Discovery engine: `gh` (default) or `forgejo`.
    list_engine: String,
    /// Discovery filter: a GitHub search query, or Forgejo query-string
    /// qualifiers. Empty means the engine's default.
    list_filter: String,
    /// The Forgejo host discovery searches (the `forgejo` engine needs
    /// one; requests are host-scoped).
    list_host: String,
    /// Named discovery filters, pickable in the review queue.
    list_filters: BTreeMap<String, String>,
    /// `owner/repo` → the local clone that holds it, for local editing.
    clones: BTreeMap<String, String>,
    /// The editor template for opening a file locally: a URL or a
    /// command, with {path}, {line}, {dir}. Empty means textchum.
    editor_command: String,
    /// The named keymap `keys` overlays (from `keymaps`).
    keymap: String,
    /// User-defined named keymaps: name → {action → key spec}.
    keymaps: BTreeMap<String, BTreeMap<String, String>>,
    /// Syntax theme name: a built-in or a themes/<name>.json file.
    theme: String,
    /// `system` (default) | `light` | `dark`.
    appearance: String,
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
        Self::read_string_map(object, "keys", &mut config.keys, &mut config.load_warning);
        if let Some(value) = object.get("keymaps") {
            match value.as_object() {
                Some(maps) => {
                    for (name, entries) in maps {
                        let mut map = BTreeMap::new();
                        if let Some(entries) = entries.as_object() {
                            for (action, spec) in entries {
                                if let Some(spec) = spec.as_str() {
                                    map.insert(action.clone(), spec.to_string());
                                } else {
                                    config.load_warning = Some(format!(
                                        "keymaps.{name}.{action} must be a string; ignored"
                                    ));
                                }
                            }
                        } else {
                            config.load_warning = Some(format!(
                                "keymaps.{name} must be an object of action → key spec"
                            ));
                        }
                        config.keymaps.insert(name.clone(), map);
                    }
                }
                None => {
                    config.load_warning =
                        Some("keymaps must be an object of named keymaps".to_string());
                }
            }
        }
        if let Some(value) = object.get("keymap") {
            match value.as_str() {
                Some(name) => config.keymap = name.to_string(),
                None => {
                    config.load_warning = Some("keymap must be a string; ignored".to_string());
                }
            }
        }
        Self::read_string_map(object, "forges", &mut config.forges, &mut config.load_warning);
        Self::read_string_map(
            object,
            "list_filters",
            &mut config.list_filters,
            &mut config.load_warning,
        );
        Self::read_string_map(object, "clones", &mut config.clones, &mut config.load_warning);
        for (key, target) in [
            ("forgejo_api_command", 0usize),
            ("list_engine", 1),
            ("list_filter", 2),
            ("list_host", 3),
            ("theme", 4),
            ("appearance", 5),
            ("editor_command", 6),
        ] {
            let Some(value) = object.get(key) else { continue };
            match value.as_str() {
                Some(text) => match target {
                    0 => config.forgejo_api_command = text.to_string(),
                    1 => config.list_engine = text.to_string(),
                    2 => config.list_filter = text.to_string(),
                    3 => config.list_host = text.to_string(),
                    4 => config.theme = text.to_string(),
                    5 => config.appearance = text.to_string(),
                    _ => config.editor_command = text.to_string(),
                },
                None => {
                    config.load_warning = Some(format!("{key} must be a string; ignored"));
                }
            }
        }
        config
    }

    fn read_string_map(
        object: &serde_json::Map<String, serde_json::Value>,
        name: &str,
        into: &mut BTreeMap<String, String>,
        warning: &mut Option<String>,
    ) {
        let Some(value) = object.get(name) else {
            return;
        };
        let Some(map) = value.as_object() else {
            *warning = Some(format!("{name} must be an object of string → string"));
            return;
        };
        for (key, entry) in map {
            match entry.as_str() {
                Some(text) => {
                    into.insert(key.clone(), text.to_string());
                }
                None => {
                    *warning = Some(format!("{name}.{key} must be a string; ignored"));
                }
            }
        }
    }

    fn warned(message: String) -> Self {
        Self {
            load_warning: Some(message),
            ..Default::default()
        }
    }

    /// The problem found while loading, if any. Defaults are in effect
    /// when this is set; the file on disk was left untouched.
    pub fn load_warning(&self) -> Option<&str> {
        self.load_warning.as_deref()
    }

    /// The effective key-binding overrides as a JSON object string: the
    /// selected named keymap (when any), with top-level `keys` on top —
    /// overrides of overrides, textchum-style.
    pub fn keys_json(&self) -> String {
        let mut effective: BTreeMap<String, String> = BTreeMap::new();
        if !self.keymap.is_empty() {
            match self.keymaps.get(&self.keymap) {
                Some(base) => effective.extend(base.clone()),
                None => {
                    // Reported through keys the shell logs anyway: an
                    // unknown name simply contributes nothing.
                }
            }
        }
        effective.extend(self.keys.clone());
        serde_json::to_string(&effective).unwrap_or_else(|_| "{}".to_string())
    }

    /// The selected keymap name, and whether it exists (for warnings).
    pub fn keymap_status(&self) -> (String, bool) {
        (
            self.keymap.clone(),
            self.keymap.is_empty() || self.keymaps.contains_key(&self.keymap),
        )
    }

    /// The configured forge kind for `host`, if any.
    pub fn forge_for_host(&self, host: &str) -> Option<&str> {
        self.forges.get(host).map(String::as_str)
    }

    /// The Forgejo API command template; empty means the built-in default.
    pub fn forgejo_api_command(&self) -> &str {
        &self.forgejo_api_command
    }

    pub fn list_engine(&self) -> &str {
        if self.list_engine.is_empty() { "gh" } else { &self.list_engine }
    }

    pub fn list_filter(&self) -> &str {
        &self.list_filter
    }

    pub fn list_host(&self) -> &str {
        &self.list_host
    }

    /// The configured clones as a JSON object string.
    pub fn clones_json(&self) -> String {
        serde_json::to_string(&self.clones).unwrap_or_else(|_| "{}".to_string())
    }

    /// The local clone configured for `owner/repo`, if any. The lookup is
    /// case-insensitive: forges are, about repository names.
    pub fn clone_for(&self, slug: &str) -> Option<&str> {
        let needle = slug.to_ascii_lowercase();
        self.clones
            .iter()
            .find(|(key, _)| key.to_ascii_lowercase() == needle)
            .map(|(_, path)| path.as_str())
    }

    /// The editor template; empty means the built-in textchum default.
    pub fn editor_command(&self) -> &str {
        &self.editor_command
    }

    /// The named discovery filters as a JSON object string.
    pub fn list_filters_json(&self) -> String {
        serde_json::to_string(&self.list_filters).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn theme(&self) -> &str {
        &self.theme
    }

    /// `system` | `light` | `dark`; anything else reads as system.
    pub fn appearance(&self) -> &str {
        match self.appearance.as_str() {
            "light" => "light",
            "dark" => "dark",
            _ => "system",
        }
    }
}

/// Writes one entry of a top-level map setting (`list_filters`,
/// `forges`, `keys`…), preserving everything else in the file. An empty
/// `value` removes the entry. Same never-clobber rules as [`set_string`].
pub fn set_map_entry(
    path: &Path,
    map_key: &str,
    entry_key: &str,
    value: &str,
) -> Result<(), String> {
    let mut root = read_object(path)?;
    if !root[map_key].is_object() {
        root[map_key] = serde_json::Value::Object(Default::default());
    }
    let map = root[map_key].as_object_mut().expect("just ensured");
    if value.is_empty() {
        map.remove(entry_key);
    } else {
        map.insert(
            entry_key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
    write_object(path, &root)
}

fn read_object(path: &Path) -> Result<serde_json::Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let parsed: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| format!("config is not valid JSON, not touching it: {e}"))?;
            if !parsed.is_object() {
                return Err("config is not a JSON object, not touching it".to_string());
            }
            Ok(parsed)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(serde_json::Value::Object(Default::default()))
        }
        Err(error) => Err(format!("could not read {}: {error}", path.display())),
    }
}

fn write_object(path: &Path, root: &serde_json::Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    }
    let mut text = serde_json::to_string_pretty(root)
        .map_err(|error| format!("could not encode config: {error}"))?;
    text.push('\n');
    crate::review::atomic_write(path, text.as_bytes())
}

/// Writes one string setting into the config file, preserving everything
/// else — unknown keys included. A file that is not a JSON object (or is
/// broken) is left untouched, matching the never-clobber rule; a missing
/// file starts fresh.
pub fn set_string(path: &Path, key: &str, value: &str) -> Result<(), String> {
    let mut root = read_object(path)?;
    root[key] = serde_json::Value::String(value.to_string());
    write_object(path, &root)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keys: BTreeMap::new(),
            keymap: String::new(),
            keymaps: BTreeMap::new(),
            forges: BTreeMap::new(),
            forgejo_api_command: String::new(),
            list_engine: String::new(),
            list_filter: String::new(),
            list_host: String::new(),
            list_filters: BTreeMap::new(),
            clones: BTreeMap::new(),
            editor_command: String::new(),
            theme: String::new(),
            appearance: String::new(),
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
    fn clones_and_editor() {
        let config = Config::from_json(
            r#"{"clones": {"Owner/Repo": "/src/repo"},
                "editor_command": "code -g {path}:{line}"}"#,
        );
        assert!(config.load_warning().is_none());
        assert_eq!(config.clone_for("owner/repo"), Some("/src/repo"));
        assert_eq!(config.clone_for("other/repo"), None);
        assert_eq!(config.editor_command(), "code -g {path}:{line}");
        assert!(Config::default().editor_command().is_empty());
    }

    #[test]
    fn named_filters_and_map_entries() {
        let config = Config::from_json(
            r#"{"list_filters": {"bugs": "is:open label:bug", "team": "is:open team-review-requested:x/y"}}"#,
        );
        assert!(config.load_warning().is_none());
        assert!(config.list_filters_json().contains("label:bug"));

        let dir = std::env::temp_dir().join(format!("prchum-flt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"future": 1}"#).unwrap();
        set_map_entry(&path, "list_filters", "bugs", "is:open label:bug").unwrap();
        set_map_entry(&path, "list_filters", "old", "x").unwrap();
        set_map_entry(&path, "list_filters", "old", "").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("future"), "{text}");
        let loaded = Config::from_json(&text);
        assert_eq!(
            loaded.list_filters_json(),
            r#"{"bugs":"is:open label:bug"}"#
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn named_keymaps_overlay() {
        let config = Config::from_json(
            r#"{"keymap": "mine",
                "keymaps": {"mine": {"next-hunk": "cmd+alt+n", "reply": "cmd+alt+r"}},
                "keys": {"reply": "cmd+shift+r"}}"#,
        );
        assert!(config.load_warning().is_none());
        // The named map is the base; top-level keys win over it.
        assert_eq!(
            config.keys_json(),
            r#"{"next-hunk":"cmd+alt+n","reply":"cmd+shift+r"}"#
        );
        assert_eq!(config.keymap_status(), ("mine".to_string(), true));

        let missing = Config::from_json(r#"{"keymap": "ghost"}"#);
        assert_eq!(missing.keymap_status(), ("ghost".to_string(), false));
        assert_eq!(missing.keys_json(), "{}");
    }

    #[test]
    fn forge_settings_load() {
        let config = Config::from_json(
            r#"{"forges": {"git.corp.example": "forgejo"},
                "forgejo_api_command": "curl -sf https://{host}/api/v1{path}"}"#,
        );
        assert!(config.load_warning().is_none());
        assert_eq!(config.forge_for_host("git.corp.example"), Some("forgejo"));
        assert_eq!(config.forge_for_host("elsewhere"), None);
        assert!(config.forgejo_api_command().starts_with("curl"));
        assert!(Config::default().forgejo_api_command().is_empty());
    }

    #[test]
    fn set_string_preserves_unknown_keys() {
        let dir = std::env::temp_dir().join(format!("prchum-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"future": [1, 2], "theme": "old"}"#).unwrap();
        set_string(&path, "theme", "high-contrast").unwrap();
        set_string(&path, "appearance", "dark").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("future"), "{text}");
        let config = Config::from_json(&text);
        assert_eq!(config.theme(), "high-contrast");
        assert_eq!(config.appearance(), "dark");
        // Broken files stay untouched.
        std::fs::write(&path, "{ broken").unwrap();
        assert!(set_string(&path, "theme", "x").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_top_level_keys_are_tolerated() {
        let config = Config::from_json(r#"{"future_setting": true, "keys": {"open": "cmd+o"}}"#);
        assert!(config.load_warning().is_none());
        assert_eq!(config.keys_json(), r#"{"open":"cmd+o"}"#);
    }
}
