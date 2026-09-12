//! User configuration: `~/.config/remtui/config.toml` (XDG), shared with the
//! Python remtui. Same tables, same lenient coercion, same default file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::Value;

use crate::util::{expand_tilde, home_dir};

pub const APP_NAME: &str = "remtui";

pub const DEFAULT_TOML: &str = r#"# remtui configuration.

[keys]
# Key profile: "default", or "vim" for extra vim motions (gg, ctrl+d/u/f/b,
# ":" for the command palette, "o" to add a reminder). The --vim flag and
# the REMTUI_KEYS environment variable override this setting.
profile = "default"
# Per-binding overrides by binding id (see the README for the id list).
# An override replaces the binding's keys entirely; comma-separate to keep
# several, e.g.:
# "reminder.add" = "a,n"

[bear]
# Triage the open todos in your Bear notes from remtui (the `b` key). Off by
# default; needs bearcli, which ships inside Bear.app.
enabled = false
# Reminders list that triaged todos are added to; "" means the list shown
# when you press `b`.
list = ""
# remctl due phrase applied to every added todo.
due = "today"
# Only look at notes under these tags; [] means every note with an open todo.
tags = []
# Path to the bearcli binary; "" means $REMTUI_BEARCLI, else bearcli on PATH.
bearcli = ""
"#;

/// The `[bear]` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BearConfig {
    pub enabled: bool,
    pub list: String,
    pub due: String,
    pub tags: Vec<String>,
    pub bearcli: String,
}

impl Default for BearConfig {
    fn default() -> Self {
        BearConfig {
            enabled: false,
            list: String::new(),
            due: "today".into(),
            tags: Vec::new(),
            bearcli: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub profile: String,
    pub overrides: BTreeMap<String, String>,
    pub bear: BearConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            profile: "default".into(),
            overrides: BTreeMap::new(),
            bear: BearConfig::default(),
        }
    }
}

/// `$XDG_CONFIG_HOME/remtui/config.toml`, else `~/.config/remtui/config.toml`.
pub fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| expand_tilde(&v))
        .unwrap_or_else(|| home_dir().join(".config"));
    base.join(APP_NAME).join("config.toml")
}

fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Integer(i)) => i.to_string(),
        Some(Value::Float(f)) => f.to_string(),
        Some(Value::Boolean(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// Python `bool(value)`.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Boolean(b)) => *b,
        Some(Value::Integer(i)) => *i != 0,
        Some(Value::Float(f)) => *f != 0.0,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Table(t)) => !t.is_empty(),
        Some(Value::Datetime(_)) => true,
        None => false,
    }
}

fn parse_bear(section: Option<&Value>) -> BearConfig {
    let Some(Value::Table(table)) = section else {
        return BearConfig::default();
    };
    let raw_tags: Vec<Value> = match table.get("tags") {
        Some(Value::String(s)) => vec![Value::String(s.clone())],
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    let tags = raw_tags
        .iter()
        .map(|t| text(Some(t)).trim().trim_matches('#').to_string())
        .filter(|t| !t.is_empty())
        .collect();
    let due = text(table.get("due")).trim().to_string();
    BearConfig {
        enabled: truthy(table.get("enabled")),
        list: text(table.get("list")),
        due: if due.is_empty() { "today".into() } else { due },
        tags,
        bearcli: text(table.get("bearcli")),
    }
}

/// Parse a config document; anything malformed falls back to defaults.
pub fn parse(source: &str) -> Config {
    let Ok(data) = source.parse::<toml::Table>() else {
        return Config::default();
    };
    let (profile, overrides) = match data.get("keys") {
        Some(Value::Table(keys)) => {
            let profile = keys
                .get("profile")
                .map(|v| text(Some(v)))
                .unwrap_or_else(|| "default".into());
            let overrides = keys
                .iter()
                .filter(|(k, v)| k.as_str() != "profile" && v.is_str())
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect();
            (profile, overrides)
        }
        _ => ("default".into(), BTreeMap::new()),
    };
    Config {
        profile,
        overrides,
        bear: parse_bear(data.get("bear")),
    }
}

/// Read the config file, writing a commented default on first run. Unreadable
/// or malformed config degrades to defaults rather than blocking startup.
pub fn load_from(path: &Path) -> Config {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, DEFAULT_TOML);
        return Config::default();
    }
    match std::fs::read_to_string(path) {
        Ok(source) => parse(&source),
        Err(_) => Config::default(),
    }
}

pub fn load() -> Config {
    load_from(&config_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_parses_to_defaults() {
        assert_eq!(parse(DEFAULT_TOML), Config::default());
    }

    #[test]
    fn keys_and_overrides() {
        let c = parse("[keys]\nprofile = \"vim\"\n\"reminder.add\" = \"a,n\"\nbogus = 3\n");
        assert_eq!(c.profile, "vim");
        assert_eq!(
            c.overrides.get("reminder.add").map(String::as_str),
            Some("a,n")
        );
        assert!(!c.overrides.contains_key("bogus"));
    }

    #[test]
    fn bear_section_is_lenient() {
        let c = parse(
            "[bear]\nenabled = 1\nlist = \"Work\"\ndue = \"  \"\ntags = [\"#work\", \" home/garden \", \"\"]\n",
        );
        assert!(c.bear.enabled);
        assert_eq!(c.bear.list, "Work");
        assert_eq!(c.bear.due, "today");
        assert_eq!(c.bear.tags, vec!["work", "home/garden"]);
        let single = parse("[bear]\ntags = \"work\"\n");
        assert_eq!(single.bear.tags, vec!["work"]);
        assert_eq!(parse("not toml ][").bear, BearConfig::default());
    }

    #[test]
    fn missing_file_is_written_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("config.toml");
        assert_eq!(load_from(&path), Config::default());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT_TOML);
        std::fs::write(&path, "[keys]\nprofile = \"vim\"\n").unwrap();
        assert_eq!(load_from(&path).profile, "vim");
    }
}
