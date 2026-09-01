//! Installed mods, read from disk.
//!
//! Scan the workshop content directory and read the `name` field from each
//! mod's `meta.cpp`. Disk owns mod names; it does not carry dependencies.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;

/// Whether an id is a Workshop mod or a DLC entry, and which kind of DLC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Mod,
    Cdlc,
    Contact,
}

/// One installed mod, or one installed DLC represented the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mod {
    /// Workshop published id for a mod, or the DLC's app id, as a string.
    pub id: String,
    /// Display name, read from meta.cpp (mod), mod.cpp (CDLC), or fixed
    /// (Contact).
    pub name: String,
    /// Absolute path to the mod or DLC directory.
    pub path: PathBuf,
    pub kind: ItemKind,
    /// What a -mod= entry uses. Only ever Some for Cdlc/Contact.
    pub folder_name: Option<String>,
}

fn entry_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+?);\s*$"#).expect("static pattern")
    })
}

/// Parse an Arma config file of simple `key = value;` lines, as found in
/// meta.cpp. Quoted values are unquoted, everything else is kept verbatim.
pub fn parse_config(text: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for capture in entry_regex().captures_iter(text) {
        let key = capture[1].to_string();
        let raw = capture[2].trim_end_matches('\r');
        let value = if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
            &raw[1..raw.len() - 1]
        } else {
            raw
        };
        result.insert(key, value.to_string());
    }
    result
}

/// Turn a directory name and its meta.cpp contents into a Mod.
pub fn to_mod(dir_name: &str, path: PathBuf, meta: &HashMap<String, String>) -> Mod {
    Mod {
        id: meta.get("publishedid").cloned().unwrap_or_else(|| dir_name.to_string()),
        name: meta.get("name").cloned().unwrap_or_else(|| dir_name.to_string()),
        path,
        kind: ItemKind::Mod,
        folder_name: None,
    }
}

/// Find a directory entry matching `name` case-insensitively, since a
/// case-sensitive filesystem (most Linux setups) will not match a literal
/// `dir.join(name)` against a differently-cased real file.
fn find_case_insensitive(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|entry_name| entry_name.eq_ignore_ascii_case(name))
            .then(|| entry.path())
    })
}

/// List every mod directory under the workshop content directory. A missing
/// directory is an empty list rather than an error, since an unverified path
/// guess is a normal state.
pub fn list_mods(workshop_dir: &Path) -> Vec<Mod> {
    let Ok(entries) = std::fs::read_dir(workshop_dir) else {
        return Vec::new();
    };

    let mut mods: Vec<Mod> = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let meta = find_case_insensitive(&path, "meta.cpp")
            .and_then(|meta_path| std::fs::read_to_string(meta_path).ok())
            .map(|text| parse_config(&text))
            .unwrap_or_default();
        mods.push(to_mod(&dir_name, path, &meta));
    }

    // Case insensitive, to match the Node implementation's localeCompare so
    // both list the same order.
    mods.sort_by_key(|item| item.name.to_lowercase());
    mods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquotes_strings_and_keeps_numbers_verbatim() {
        let parsed = parse_config("protocol = 1;\npublishedid = 450814997;\nname = \"CBA_A3\";\n");
        assert_eq!(parsed.get("protocol").map(String::as_str), Some("1"));
        assert_eq!(parsed.get("publishedid").map(String::as_str), Some("450814997"));
        assert_eq!(parsed.get("name").map(String::as_str), Some("CBA_A3"));
    }

    #[test]
    fn skips_blank_and_malformed_lines() {
        let parsed = parse_config("\nnot a config line\n  name = \"Misery\";\n");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed.get("name").map(String::as_str), Some("Misery"));
    }

    #[test]
    fn handles_windows_line_endings() {
        let parsed = parse_config("name = \"Zeus Enhanced\";\r\npublishedid = 1779063631;\r\n");
        assert_eq!(parsed.get("name").map(String::as_str), Some("Zeus Enhanced"));
        assert_eq!(parsed.get("publishedid").map(String::as_str), Some("1779063631"));
    }

    #[test]
    fn prefers_meta_values_over_the_directory_name() {
        let mut meta = HashMap::new();
        meta.insert("publishedid".into(), "450814997".into());
        meta.insert("name".into(), "CBA_A3".into());
        let item = to_mod("450814997", PathBuf::from("/mods/450814997"), &meta);
        assert_eq!(item.id, "450814997");
        assert_eq!(item.name, "CBA_A3");
    }

    #[test]
    fn falls_back_to_the_directory_name_when_meta_is_unreadable() {
        let item = to_mod("2738615029", PathBuf::from("/mods/2738615029"), &HashMap::new());
        assert_eq!(item.id, "2738615029");
        assert_eq!(item.name, "2738615029");
    }

    #[test]
    fn a_missing_workshop_directory_lists_nothing() {
        assert!(list_mods(Path::new("/nonexistent/workshop/content/107410")).is_empty());
    }
}
