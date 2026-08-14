//! The persisted dependency cache.
//!
//! Version 2 `cache.json` with two sections. `entries` holds items that are
//! not installed, and is dropped for any id that is now installed so disk
//! always owns names. `installedDeps` holds dependency lists for installed
//! mods, which disk cannot provide.
//!
//! Same file the Node implementation reads and writes, so the field names
//! here are its camelCase names rather than Rust conventions.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An item that is not installed. Name known, dependencies possibly not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEntry {
    pub name: String,
    /// `None` means the name is known but the dependencies are not, which
    /// happens when a child's name is learned from its parent's page.
    pub requires: Option<Vec<String>>,
    /// ISO timestamp of the fetch, empty when only the name is known.
    pub fetched_at: String,
}

/// Dependencies of an installed mod. Its name comes from disk while it is
/// installed, but is recorded here as well so an uninstall can demote the
/// whole record into `entries` rather than losing the name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepsEntry {
    /// Absent in files written by the Node implementation, which does not
    /// record it. Those entries cannot be demoted, only dropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub requires: Vec<String>,
    pub fetched_at: String,
}

/// The cache as held in memory. BTreeMap so the written file has stable order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cache {
    pub entries: BTreeMap<String, CacheEntry>,
    pub installed_deps: BTreeMap<String, DepsEntry>,
}

/// The on-disk shape. Sections are read loosely so one malformed entry cannot
/// discard the whole file, matching the Node implementation's tolerance.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheFile {
    #[serde(default)]
    entries: BTreeMap<String, Value>,
    #[serde(default)]
    installed_deps: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheFileOut<'a> {
    version: u32,
    entries: &'a BTreeMap<String, CacheEntry>,
    installed_deps: &'a BTreeMap<String, DepsEntry>,
}

fn strings_of(value: Option<&Value>) -> Option<Vec<String>> {
    let array = value?.as_array()?;
    Some(array.iter().filter_map(|item| item.as_str().map(str::to_string)).collect())
}

fn fetched_at_of(value: &Value) -> String {
    value.get("fetchedAt").and_then(Value::as_str).unwrap_or_default().to_string()
}

/// Build the in-memory cache from parsed JSON. Entries for ids that are now
/// installed are dropped, since disk owns those names. Installed dependency
/// lists survive, but only while their mod is still installed.
pub fn merge_cache(raw: &Value, installed_ids: &HashSet<String>) -> Cache {
    let file: CacheFile = serde_json::from_value(raw.clone()).unwrap_or_default();
    let mut cache = Cache::default();

    for (id, value) in &file.entries {
        if installed_ids.contains(id) {
            continue;
        }
        let Some(name) = value.get("name").and_then(Value::as_str) else {
            continue;
        };
        let requires = match value.get("requires") {
            None | Some(Value::Null) => None,
            Some(Value::Array(_)) => strings_of(value.get("requires")),
            // Present but not an array or null: malformed, skip the entry.
            Some(_) => continue,
        };
        cache.entries.insert(
            id.clone(),
            CacheEntry { name: name.to_string(), requires, fetched_at: fetched_at_of(value) },
        );
    }

    for (id, value) in &file.installed_deps {
        let Some(requires) = strings_of(value.get("requires")) else {
            continue;
        };
        let name = value.get("name").and_then(Value::as_str).map(str::to_string);
        let fetched_at = fetched_at_of(value);

        if installed_ids.contains(id) {
            cache.installed_deps.insert(id.clone(), DepsEntry { name, requires, fetched_at });
            continue;
        }

        // No longer installed, so disk no longer owns the name. Demote the
        // record into `entries` rather than discarding what we know. Without a
        // recorded name there is nothing to demote, so the record is dropped.
        if let Some(name) = name
            && !cache.entries.contains_key(id)
        {
            cache
                .entries
                .insert(id.clone(), CacheEntry { name, requires: Some(requires), fetched_at });
        }
    }

    cache
}

/// Load the persisted cache. A missing or corrupt file is simply empty.
pub fn load_cache(installed_ids: &HashSet<String>, file: &Path) -> Cache {
    match std::fs::read_to_string(file).ok().and_then(|text| serde_json::from_str(&text).ok()) {
        Some(value) => merge_cache(&value, installed_ids),
        None => Cache::default(),
    }
}

/// Write the cache back out, creating the config directory if needed.
pub fn save_cache(cache: &Cache, file: &PathBuf) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = CacheFileOut {
        version: 2,
        entries: &cache.entries,
        installed_deps: &cache.installed_deps,
    };
    let mut text = serde_json::to_string_pretty(&out)?;
    text.push('\n');
    std::fs::write(file, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "version": 2,
            "entries": {
                "2888888564": {
                    "name": "Advanced Equipment",
                    "requires": ["450814997"],
                    "fetchedAt": "2026-08-13T00:00:00.000Z"
                },
                "450814997": { "name": "CBA_A3", "requires": null, "fetchedAt": "" }
            },
            "installedDeps": {
                "463939057": {
                    "name": "ace",
                    "requires": ["450814997"],
                    "fetchedAt": "2026-08-13T00:00:00.000Z"
                },
                "999999999": { "requires": ["450814997"], "fetchedAt": "2026-08-13T00:00:00.000Z" }
            }
        })
    }

    fn ids(list: &[&str]) -> HashSet<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn drops_entries_for_ids_that_are_now_installed() {
        let cache = merge_cache(&sample(), &ids(&["450814997"]));
        assert!(!cache.entries.contains_key("450814997"), "installed ids lose their cached name");
        assert!(cache.entries.contains_key("2888888564"), "uninstalled ids keep theirs");
    }

    #[test]
    fn keeps_a_null_requires_meaning_name_known_deps_unknown() {
        let cache = merge_cache(&sample(), &ids(&[]));
        let entry = cache.entries.get("450814997").expect("entry present");
        assert_eq!(entry.name, "CBA_A3");
        assert_eq!(entry.requires, None);
    }

    #[test]
    fn keeps_installed_dependency_lists_across_a_restart() {
        let cache = merge_cache(&sample(), &ids(&["463939057"]));
        assert_eq!(
            cache.installed_deps.get("463939057").map(|entry| entry.requires.clone()),
            Some(vec!["450814997".to_string()])
        );
    }

    #[test]
    fn drops_installed_deps_for_mods_no_longer_installed() {
        let cache = merge_cache(&sample(), &ids(&["463939057"]));
        assert!(!cache.installed_deps.contains_key("999999999"));
    }

    /// Uninstalling a mod must not cost us its name. The record moves into
    /// `entries`, where a not-installed item belongs.
    #[test]
    fn demotes_a_name_into_entries_when_the_mod_is_uninstalled() {
        let cache = merge_cache(&sample(), &ids(&[]));
        let demoted = cache.entries.get("463939057").expect("demoted into entries");
        assert_eq!(demoted.name, "ace");
        assert_eq!(demoted.requires, Some(vec!["450814997".to_string()]));
        assert!(!cache.installed_deps.contains_key("463939057"));
    }

    /// A file written by the Node implementation has no name to demote, so
    /// that record is dropped rather than invented.
    #[test]
    fn a_nameless_deps_record_is_dropped_rather_than_invented() {
        let cache = merge_cache(&sample(), &ids(&[]));
        assert!(!cache.entries.contains_key("999999999"));
    }

    /// An existing entry already describes the item, so demotion must not
    /// overwrite it.
    #[test]
    fn demotion_does_not_overwrite_an_existing_entry() {
        let raw = json!({
            "entries": { "463939057": { "name": "from entries", "requires": null, "fetchedAt": "" } },
            "installedDeps": { "463939057": { "name": "ace", "requires": [], "fetchedAt": "" } }
        });
        let cache = merge_cache(&raw, &ids(&[]));
        assert_eq!(cache.entries.get("463939057").map(|entry| entry.name.clone()), Some("from entries".into()));
    }

    #[test]
    fn ignores_malformed_entries_rather_than_failing() {
        let raw = json!({
            "version": 2,
            "entries": {
                "a": { "name": 5 },
                "b": { "name": "ok", "requires": "nope" },
                "c": { "name": "fine", "requires": [] }
            }
        });
        let cache = merge_cache(&raw, &ids(&[]));
        assert_eq!(cache.entries.keys().collect::<Vec<_>>(), vec!["c"]);
    }

    #[test]
    fn treats_a_missing_or_unparsable_file_as_empty() {
        let cache = merge_cache(&Value::Null, &ids(&[]));
        assert!(cache.entries.is_empty());
        assert!(cache.installed_deps.is_empty());
        assert_eq!(load_cache(&ids(&[]), Path::new("/nonexistent/cache.json")), Cache::default());
    }

    /// The file must stay readable by the Node implementation, so the shape
    /// and the camelCase names have to survive a round trip.
    #[test]
    fn round_trips_through_the_node_file_shape() {
        let cache = merge_cache(&sample(), &ids(&["463939057"]));
        let out = CacheFileOut {
            version: 2,
            entries: &cache.entries,
            installed_deps: &cache.installed_deps,
        };
        let text = serde_json::to_string(&out).expect("serializes");
        assert!(text.contains("\"installedDeps\""));
        assert!(text.contains("\"fetchedAt\""));

        let reparsed: Value = serde_json::from_str(&text).expect("parses");
        assert_eq!(merge_cache(&reparsed, &ids(&["463939057"])), cache);
    }
}
