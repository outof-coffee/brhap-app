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

/// What is known about one id. One id, one variant: installed and
/// not-installed can never both be true for the same id at once, since
/// there is exactly one record per id rather than two maps that could
/// disagree about which of them holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheStatus {
    Installed(DepsEntry),
    NotInstalled(CacheEntry),
}

/// The cache as held in memory. BTreeMap so the written file has stable order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cache {
    records: BTreeMap<String, CacheStatus>,
}

impl Cache {
    /// What is known about one id.
    pub fn status(&self, id: &str) -> Option<CacheStatus> {
        self.records.get(id).cloned()
    }

    /// Every id currently recorded, in no particular order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.records.keys().map(String::as_str)
    }

    /// Record what is known about one id. `pub(crate)`: `resolve.rs` writes
    /// through this the same way `merge_cache` does; nothing outside this
    /// crate should be able to bypass `status()`'s single-record guarantee.
    pub(crate) fn insert(&mut self, id: String, status: CacheStatus) {
        self.records.insert(id, status);
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.records.contains_key(id)
    }
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
        cache.insert(
            id.clone(),
            CacheStatus::NotInstalled(CacheEntry {
                name: name.to_string(),
                requires,
                fetched_at: fetched_at_of(value),
            }),
        );
    }

    for (id, value) in &file.installed_deps {
        let Some(requires) = strings_of(value.get("requires")) else {
            continue;
        };
        let name = value.get("name").and_then(Value::as_str).map(str::to_string);
        let fetched_at = fetched_at_of(value);

        if installed_ids.contains(id) {
            cache.insert(id.clone(), CacheStatus::Installed(DepsEntry { name, requires, fetched_at }));
            continue;
        }

        // No longer installed, so disk no longer owns the name. Demote the
        // record into a not-installed one rather than discarding what we
        // know. Without a recorded name there is nothing to demote, so the
        // record is dropped.
        if let Some(name) = name
            && !cache.contains(id)
        {
            cache.insert(
                id.clone(),
                CacheStatus::NotInstalled(CacheEntry { name, requires: Some(requires), fetched_at }),
            );
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

/// Write the cache back out, creating the config directory if needed. Splits
/// the one in-memory map back into the two on-disk sections the Node
/// implementation expects.
pub fn save_cache(cache: &Cache, file: &PathBuf) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut entries = BTreeMap::new();
    let mut installed_deps = BTreeMap::new();
    for (id, status) in &cache.records {
        match status {
            CacheStatus::NotInstalled(entry) => {
                entries.insert(id.clone(), entry.clone());
            }
            CacheStatus::Installed(deps) => {
                installed_deps.insert(id.clone(), deps.clone());
            }
        }
    }

    let out = CacheFileOut { version: 2, entries: &entries, installed_deps: &installed_deps };
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
        assert_eq!(cache.status("450814997"), None, "installed ids lose their cached name");
        assert!(cache.status("2888888564").is_some(), "uninstalled ids keep theirs");
    }

    #[test]
    fn keeps_a_null_requires_meaning_name_known_deps_unknown() {
        let cache = merge_cache(&sample(), &ids(&[]));
        match cache.status("450814997") {
            Some(CacheStatus::NotInstalled(entry)) => {
                assert_eq!(entry.name, "CBA_A3");
                assert_eq!(entry.requires, None);
            }
            other => panic!("expected a not-installed record, got {other:?}"),
        }
    }

    #[test]
    fn keeps_installed_dependency_lists_across_a_restart() {
        let cache = merge_cache(&sample(), &ids(&["463939057"]));
        match cache.status("463939057") {
            Some(CacheStatus::Installed(entry)) => {
                assert_eq!(entry.requires, vec!["450814997".to_string()]);
            }
            other => panic!("expected an installed record, got {other:?}"),
        }
    }

    #[test]
    fn drops_installed_deps_for_mods_no_longer_installed() {
        let cache = merge_cache(&sample(), &ids(&["463939057"]));
        assert!(!matches!(cache.status("999999999"), Some(CacheStatus::Installed(_))));
    }

    /// Uninstalling a mod must not cost us its name. The record moves into a
    /// not-installed one, where it belongs once nothing is installed.
    #[test]
    fn demotes_a_name_into_entries_when_the_mod_is_uninstalled() {
        let cache = merge_cache(&sample(), &ids(&[]));
        match cache.status("463939057") {
            Some(CacheStatus::NotInstalled(entry)) => {
                assert_eq!(entry.name, "ace");
                assert_eq!(entry.requires, Some(vec!["450814997".to_string()]));
            }
            other => panic!("expected demotion into a not-installed record, got {other:?}"),
        }
    }

    /// A file written by the Node implementation has no name to demote, so
    /// that record is dropped rather than invented.
    #[test]
    fn a_nameless_deps_record_is_dropped_rather_than_invented() {
        let cache = merge_cache(&sample(), &ids(&[]));
        assert_eq!(cache.status("999999999"), None);
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
        match cache.status("463939057") {
            Some(CacheStatus::NotInstalled(entry)) => assert_eq!(entry.name, "from entries"),
            other => panic!("expected the original entries record, got {other:?}"),
        }
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
        assert_eq!(cache.ids().collect::<Vec<_>>(), vec!["c"]);
    }

    #[test]
    fn treats_a_missing_or_unparsable_file_as_empty() {
        let cache = merge_cache(&Value::Null, &ids(&[]));
        assert_eq!(cache.ids().count(), 0);
        assert_eq!(load_cache(&ids(&[]), Path::new("/nonexistent/cache.json")), Cache::default());
    }

    /// The file must stay readable by the Node implementation, so the shape
    /// and the camelCase names have to survive a round trip.
    #[test]
    fn round_trips_through_the_node_file_shape() {
        let cache = merge_cache(&sample(), &ids(&["463939057"]));
        let file = std::env::temp_dir().join("brhap-server-cache-round-trip-test.json");
        save_cache(&cache, &file).expect("saves");

        let text = std::fs::read_to_string(&file).expect("file written");
        assert!(text.contains("\"installedDeps\""));
        assert!(text.contains("\"fetchedAt\""));

        let reloaded = load_cache(&ids(&["463939057"]), &file);
        assert_eq!(reloaded, cache);
        std::fs::remove_file(&file).ok();
    }

    /// Behavior test for merge_cache's output: asserted through `status()`,
    /// not through which field holds the result.
    #[test]
    fn status_reports_installed_or_not_installed_without_naming_a_field() {
        let cache = merge_cache(&sample(), &ids(&["463939057"]));

        match cache.status("463939057") {
            Some(CacheStatus::Installed(entry)) => {
                assert_eq!(entry.requires, vec!["450814997".to_string()]);
            }
            other => panic!("expected an installed record, got {other:?}"),
        }

        match cache.status("2888888564") {
            Some(CacheStatus::NotInstalled(entry)) => {
                assert_eq!(entry.name, "Advanced Equipment");
            }
            other => panic!("expected a not-installed record, got {other:?}"),
        }

        assert_eq!(cache.status("nonexistent"), None);
    }
}
