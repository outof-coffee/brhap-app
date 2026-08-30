//! The contract every brhap frontend talks to.
//!
//! `brhap-server` is the low level, blocking library. This crate sits directly
//! above it and owns the pieces a frontend would otherwise have to assemble for
//! itself: the discovered Steam paths, the resolver, the profile store, and the
//! running session. What it exposes is exactly the operation set the UI needs.
//!
//! The point is that a desktop wrapper, a native binary, and a future HTTP
//! server all describe the same API rather than agreeing by hand. Before this
//! crate existed the aggregation lived in the Tauri binary, so a second
//! frontend had to copy it.
//!
//! Blocking, like the library underneath. Callers decide for themselves how to
//! get this off their UI thread.

pub mod settings;
mod steam;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;

pub use brhap_server::launch::{LaunchOptions, LaunchPlan, Symlink};
pub use brhap_server::profiles::{LastLaunch, Profile, Profiles};
pub use brhap_server::resolve::{Resolved, Source};
pub use brhap_server::session::{Event, Launched, Listener};
pub use brhap_server::steam::{Located, SteamPaths};
pub use settings::{Settings, SettingsRow, settings_rows, stored_steam_key};
use settings::*;

/// Per-mod replacement directories, keyed by workshop id.
pub type Overrides = BTreeMap<String, std::path::PathBuf>;

/// Everything the UI needs to draw itself, with no network access at all.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub mods: Vec<Resolved>,
    pub referenced: Vec<Resolved>,
    pub api_available: bool,
    /// False when a path was guessed rather than confirmed on disk.
    pub game_verified: bool,
    pub workshop_verified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalkSummary {
    pub calls: usize,
    pub resolved: usize,
}

/// A poisoned lock means some other caller panicked mid-update. The data is
/// still structurally sound, so carry on rather than spreading the panic.
fn locked<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct Core {
    resolver: Mutex<brhap_server::resolve::Resolver>,
    session: Arc<brhap_server::session::Session>,
    paths: SteamPaths,
    profiles: Mutex<brhap_server::profiles::ProfileStore>,
    settings: Mutex<SettingsStore>
}

impl Core {
    /// Discover the Steam layout and open the cache and profile stores.
    ///
    /// The listener is handed straight to the session, so this crate never
    /// learns which frontend is listening.
    pub fn new(listener: Listener) -> Self {
        let paths = brhap_server::steam::discover();
        let resolver = brhap_server::resolve::Resolver::new(
            paths.workshop.path.clone(),
            brhap_server::config::cache_file(),
        );

        Self {
            resolver: Mutex::new(resolver),
            session: Arc::new(brhap_server::session::Session::new(listener)),
            paths,
            profiles: Mutex::new(brhap_server::profiles::ProfileStore::new(
                brhap_server::config::profiles_file(),
            )),
            settings: Mutex::new(SettingsStore::new(settings_file())),
        }
    }

    /// The discovered paths, for a frontend that wants to show them.
    pub fn paths(&self) -> &SteamPaths {
        &self.paths
    }

    pub fn snapshot(&self) -> Snapshot {
        let resolver = locked(&self.resolver);
        let mods: Vec<Resolved> =
            resolver.mods().iter().map(|item| resolver.view(&item.id)).collect();

        // Requirements of installed mods, deduplicated and in id order, so the
        // UI can name a dependency it does not have installed.
        let mut referenced: BTreeMap<String, Resolved> = BTreeMap::new();
        for item in &mods {
            for id in item.requires.iter().flatten() {
                referenced.entry(id.clone()).or_insert_with(|| resolver.view(id));
            }
        }

        Snapshot {
            mods,
            referenced: referenced.into_values().collect(),
            api_available: self.steam_key().is_some(),
            game_verified: self.paths.game.verified,
            workshop_verified: self.paths.workshop.verified,
        }
    }

    /// Re-read the workshop directory, picking up installs and removals.
    pub fn rescan(&self) -> Snapshot {
        locked(&self.resolver).rescan();
        self.snapshot()
    }

    /// Discard every cached dependency lookup. Installed names come from disk,
    /// so they are unaffected.
    pub fn reset_cache(&self) -> Snapshot {
        locked(&self.resolver).reset_cache();
        self.snapshot()
    }

    /// Fetch one Workshop page. Call only in response to a user action.
    pub fn resolve_item(&self, id: &str, refresh: bool) -> Result<Resolved, String> {
        if id.is_empty() || !id.chars().all(|character| character.is_ascii_digit()) {
            return Err("id must be a numeric workshop id".into());
        }
        locked(&self.resolver).resolve(id, refresh).map_err(|error| error.to_string())
    }

    /// The Steam Web API key, saved settings first and the environment second.
    ///
    /// The two sources are resolved here rather than in
    /// `brhap_server::api::load_steam_key`, which reads the environment and
    /// knows nothing about settings: the store lives in this crate, one layer
    /// above that one.
    pub fn steam_key(&self) -> Option<String> {
        stored_steam_key(&locked(&self.settings).view())
            .or_else(brhap_server::api::load_steam_key)
    }

    /// Batched walk over the Steam Web API. Needs a key from either source;
    /// `Snapshot::api_available` says whether there is one.
    pub fn walk_all(&self) -> Result<WalkSummary, String> {
        let Some(key) = self.steam_key() else {
            return Err(format!(
                "no Steam Web API key is saved or set in {}, the batched walk is unavailable",
                brhap_server::api::KEY_VAR
            ));
        };
        let result = locked(&self.resolver).prefill_from_api(&key)?;
        Ok(WalkSummary { calls: result.calls, resolved: result.items.len() })
    }

    /// Describe a launch without performing one.
    pub fn preview(
        &self,
        ids: &[String],
        options: LaunchOptions,
        overrides: &Overrides,
    ) -> LaunchPlan {
        brhap_server::launch::build_launch_plan(&self.paths, ids, options, overrides)
    }

    /// Link the selected mods in and spawn the game.
    pub fn launch(
        &self,
        ids: &[String],
        options: LaunchOptions,
        overrides: &Overrides,
    ) -> Result<Launched, String> {
        // try to save settings, but continue if not
        let _ = locked(&self.settings).save();
        let plan = self.preview(ids, options, overrides);
        let launched = self
            .session
            .launch(&plan, &self.paths.workshop.path)
            .map_err(|error| error.to_string())?;

        // Recorded only once the game is actually running, so a rejected launch
        // leaves the previous record alone. The game is up either way, so a
        // failed write says so rather than reading as a failed launch.
        locked(&self.profiles)
            .record_launch(ids, options, overrides)
            .map_err(|error| format!("launched, but the last launch was not recorded: {error}"))?;

        Ok(launched)
    }

    /// Ask the game to close. The exit arrives through the listener.
    pub fn stop(&self) -> Result<(), String> {
        self.session.stop().map_err(|error| error.to_string())
    }

    pub fn running(&self) -> bool {
        self.session.running()
    }

    pub fn list_profiles(&self) -> Profiles {
        locked(&self.profiles).view()
    }
    
    pub fn settings(&self) -> Settings {
        locked(&self.settings).view()
    }

    /// Each of these returns the whole store, so a caller restates rather than
    /// patches.
    pub fn save_steam_key(&self, key: &str) -> Result<Settings, String> {
        let mut store = locked(&self.settings);
        // todo: handle steam_id detection / ingress
        store.save_steam_key(0, key.to_string()).map_err(|error| error.to_string())?;
        Ok(store.view())
    }
    
    pub fn clear_steam_key(&self) -> Result<Settings, String> {
        let mut store = locked(&self.settings);
        // todo: handle steam_id detection / ingress
        store.clear_steam_key(0).map_err(|error| error.to_string())?;
        Ok(store.view())
    }

    pub fn save_profile(&self, name: &str) -> Result<Profiles, String> {
        let mut store = locked(&self.profiles);
        store.save_named(name).map_err(|error| error.to_string())?;
        Ok(store.view())
    }

    pub fn delete_profile(&self, name: &str) -> Result<Profiles, String> {
        let mut store = locked(&self.profiles);
        store.delete(name).map_err(|error| error.to_string())?;
        Ok(store.view())
    }

    // todo: save verification of paths
}
