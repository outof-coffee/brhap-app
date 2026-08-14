//! The resolve protocol.
//!
//! Names for installed mods come from disk. Names for anything else, and
//! dependency lists for everything, come from the persisted cache or from a
//! single Workshop page fetch triggered by an explicit user action. Nothing
//! here fetches on its own.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use serde::Serialize;

use crate::api::{self, WalkResult};
use crate::cache::{self, Cache, CacheEntry, DepsEntry};
use crate::mods::{self, Mod};
use crate::scrape::{self, ScrapeError};

/// Where a piece of knowledge came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Disk,
    Cache,
    Unknown,
}

/// Everything known about one workshop id.
///
/// Serialized in camelCase so the webview sees the same field names the Node
/// HTTP adapter produces, letting one frontend serve both transports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub id: String,
    /// None when nothing has resolved a name for this id yet.
    pub name: Option<String>,
    pub installed: bool,
    /// None until a fetch has told us. meta.cpp does not carry dependencies.
    pub requires: Option<Vec<String>>,
    pub source: Source,
    /// ISO timestamp behind `requires`, empty when never fetched.
    pub fetched_at: String,
}

pub struct Resolver {
    installed: BTreeMap<String, Mod>,
    cache: Cache,
    cache_path: PathBuf,
    workshop_dir: PathBuf,
}

impl Resolver {
    /// Scan disk, prefill installed names, then load the persisted cache.
    pub fn new(workshop_dir: PathBuf, cache_path: PathBuf) -> Self {
        let mut resolver =
            Self { installed: BTreeMap::new(), cache: Cache::default(), cache_path, workshop_dir };
        resolver.rescan();
        resolver
    }

    /// Re-read the workshop directory and re-merge the cache against it, so a
    /// mod installed or removed since startup is picked up.
    pub fn rescan(&mut self) {
        self.installed = mods::list_mods(&self.workshop_dir)
            .into_iter()
            .map(|item| (item.id.clone(), item))
            .collect();
        let ids: HashSet<String> = self.installed.keys().cloned().collect();
        self.cache = cache::load_cache(&ids, &self.cache_path);
    }

    /// Throw away everything learned from the network and write the empty
    /// cache out, so a restart does not resurrect it. Installed mod names are
    /// unaffected, since those come from disk.
    pub fn reset_cache(&mut self) {
        self.cache = Cache::default();
        self.save();
    }

    /// Installed mods, in the order `mods::list_mods` produced.
    pub fn mods(&self) -> Vec<Mod> {
        let mut list: Vec<Mod> = self.installed.values().cloned().collect();
        list.sort_by_key(|item| item.name.to_lowercase());
        list
    }

    /// Current knowledge about an id, without touching the network.
    pub fn view(&self, id: &str) -> Resolved {
        if let Some(item) = self.installed.get(id) {
            let deps = self.cache.installed_deps.get(id);
            return Resolved {
                id: id.to_string(),
                name: Some(item.name.clone()),
                installed: true,
                requires: deps.map(|entry| entry.requires.clone()),
                source: Source::Disk,
                fetched_at: deps.map(|entry| entry.fetched_at.clone()).unwrap_or_default(),
            };
        }

        if let Some(entry) = self.cache.entries.get(id) {
            return Resolved {
                id: id.to_string(),
                name: Some(entry.name.clone()),
                installed: false,
                requires: entry.requires.clone(),
                source: Source::Cache,
                fetched_at: entry.fetched_at.clone(),
            };
        }

        Resolved {
            id: id.to_string(),
            name: None,
            installed: false,
            requires: None,
            source: Source::Unknown,
            fetched_at: String::new(),
        }
    }

    /// Fetch one Workshop page for this id and record what it says. Call only
    /// in response to a user action. `refresh` re-fetches something known.
    pub fn resolve(&mut self, id: &str, refresh: bool) -> Result<Resolved, ScrapeError> {
        if !refresh {
            let known = self.view(id);
            if known.requires.is_some() {
                return Ok(known);
            }
        }

        let page = scrape::fetch_workshop_page(id)?;
        let fetched_at = now();

        if self.installed.contains_key(id) {
            self.cache.installed_deps.insert(
                id.to_string(),
                DepsEntry {
                    // Recorded so an uninstall can demote this into `entries`
                    // instead of losing the name disk currently owns.
                    name: self.installed.get(id).map(|item| item.name.clone()),
                    requires: page.requires.clone(),
                    fetched_at: fetched_at.clone(),
                },
            );
        } else {
            self.cache.entries.insert(
                id.to_string(),
                CacheEntry {
                    name: page.name.clone().unwrap_or_else(|| id.to_string()),
                    requires: Some(page.requires.clone()),
                    fetched_at: fetched_at.clone(),
                },
            );
        }

        // Names of the required items come free with the same page, so record
        // any that are neither installed nor already known. Their own
        // dependencies stay unknown until someone asks for them.
        for (child_id, child_name) in &page.required_names {
            if self.installed.contains_key(child_id) || self.cache.entries.contains_key(child_id) {
                continue;
            }
            self.cache.entries.insert(
                child_id.clone(),
                CacheEntry {
                    name: child_name.clone(),
                    requires: None,
                    fetched_at: String::new(),
                },
            );
        }

        self.save();
        Ok(self.view(id))
    }

    /// Optional batched path. Walks every installed mod's tree through the
    /// Steam API and fills the cache, so the click-driven scrape has nothing
    /// left to ask for.
    pub fn prefill_from_api(&mut self, key: &str) -> Result<WalkResult, String> {
        let ids: Vec<String> = self.installed.keys().cloned().collect();
        let result = api::walk_dependencies(&ids, key)?;
        let fetched_at = now();

        for (id, item) in &result.items {
            if self.installed.contains_key(id) {
                self.cache.installed_deps.insert(
                    id.clone(),
                    DepsEntry {
                        name: self.installed.get(id).map(|entry| entry.name.clone()),
                        requires: item.requires.clone(),
                        fetched_at: fetched_at.clone(),
                    },
                );
            } else {
                self.cache.entries.insert(
                    id.clone(),
                    CacheEntry {
                        name: item.name.clone(),
                        requires: Some(item.requires.clone()),
                        fetched_at: fetched_at.clone(),
                    },
                );
            }
        }

        self.save();
        Ok(result)
    }

    fn save(&self) {
        // A cache write failure is not worth losing the resolved answer over.
        let _ = cache::save_cache(&self.cache, &self.cache_path);
    }
}

/// ISO 8601 timestamp, matching what the Node implementation writes.
fn now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    format_iso(seconds)
}

fn format_iso(seconds: u64) -> String {
    let days = seconds / 86_400;
    let time = seconds % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.000Z",
        time / 3600,
        (time % 3600) / 60,
        time % 60
    )
}

/// Days since the epoch to a calendar date, by Howard Hinnant's civil_from_days.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_ids_report_nothing_rather_than_guessing() {
        let resolver = Resolver::new(
            PathBuf::from("/nonexistent/workshop"),
            PathBuf::from("/nonexistent/cache.json"),
        );
        let view = resolver.view("2888888564");
        assert_eq!(view.name, None);
        assert_eq!(view.requires, None);
        assert_eq!(view.source, Source::Unknown);
        assert!(!view.installed);
    }

    #[test]
    fn timestamps_match_the_iso_shape_node_writes() {
        assert_eq!(format_iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_iso(1_000_000_000), "2001-09-09T01:46:40.000Z");
    }

    #[test]
    fn leap_years_do_not_shift_the_date() {
        // 2024-02-29T00:00:00Z, a leap day.
        assert_eq!(format_iso(1_709_164_800), "2024-02-29T00:00:00.000Z");
    }
}
