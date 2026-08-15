//! brhap (Bohemian Rhapsody), the desktop wrapper.
//!
//! A Tauri v2 application that hosts the built web UI from ../web/dist and
//! links ../server-rs directly as a library. There is no HTTP layer here: the
//! commands below are the desktop transport, and Tauri events replace the SSE
//! stream the Node server uses.
//!
//! The library is blocking on purpose, so every command hands its work to
//! `spawn_blocking` rather than making the library async.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use brhap_server::launch::{LaunchOptions, LaunchPlan, build_launch_plan};
use brhap_server::profiles::{ProfileStore, Profiles};
use brhap_server::resolve::{Resolved, Resolver};
use brhap_server::session::{Event, Launched, Session};
use brhap_server::steam::SteamPaths;
use brhap_server::{api, config, steam};
use serde::Serialize;
use tauri::{Emitter, Manager, State};

/// Name the webview listens on for session events.
const EVENT: &str = "session-event";

struct AppState {
    resolver: Arc<Mutex<Resolver>>,
    session: Arc<Session>,
    paths: Arc<SteamPaths>,
    profiles: Arc<Mutex<ProfileStore>>,
}

/// Everything the UI needs to draw itself, with no network access at all.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    mods: Vec<Resolved>,
    referenced: Vec<Resolved>,
    api_available: bool,
    /// False when a path was guessed rather than confirmed on disk.
    game_verified: bool,
    workshop_verified: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WalkSummary {
    calls: usize,
    resolved: usize,
}

fn locked<T>(value: &Arc<Mutex<T>>) -> std::sync::MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn snapshot_of(state: &AppState) -> Snapshot {
    let resolver = locked(&state.resolver);
    let mods: Vec<Resolved> =
        resolver.mods().iter().map(|item| resolver.view(&item.id)).collect();

    let mut referenced: BTreeMap<String, Resolved> = BTreeMap::new();
    for item in &mods {
        for id in item.requires.iter().flatten() {
            referenced.entry(id.clone()).or_insert_with(|| resolver.view(id));
        }
    }

    Snapshot {
        mods,
        referenced: referenced.into_values().collect(),
        api_available: api::load_steam_key().is_some(),
        game_verified: state.paths.game.verified,
        workshop_verified: state.paths.workshop.verified,
    }
}

/// Run blocking library work off the UI thread and flatten the join error.
async fn blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work).await.map_err(|error| error.to_string())?
}

#[tauri::command]
async fn list_mods(state: State<'_, AppState>) -> Result<Snapshot, String> {
    Ok(snapshot_of(&state))
}

#[tauri::command]
async fn rescan(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let resolver = Arc::clone(&state.resolver);
    blocking(move || {
        locked(&resolver).rescan();
        Ok(())
    })
    .await?;
    Ok(snapshot_of(&state))
}

#[tauri::command]
async fn reset_cache(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let resolver = Arc::clone(&state.resolver);
    blocking(move || {
        locked(&resolver).reset_cache();
        Ok(())
    })
    .await?;
    Ok(snapshot_of(&state))
}

#[tauri::command]
async fn resolve_item(
    state: State<'_, AppState>,
    id: String,
    refresh: bool,
) -> Result<Resolved, String> {
    if !id.chars().all(|character| character.is_ascii_digit()) || id.is_empty() {
        return Err("id must be a numeric workshop id".into());
    }
    let resolver = Arc::clone(&state.resolver);
    blocking(move || locked(&resolver).resolve(&id, refresh).map_err(|error| error.to_string()))
        .await
}

#[tauri::command]
async fn walk_all(state: State<'_, AppState>) -> Result<WalkSummary, String> {
    let Some(key) = api::load_steam_key() else {
        return Err(format!("{} is not set, the batched walk is unavailable", api::KEY_VAR));
    };
    let resolver = Arc::clone(&state.resolver);
    blocking(move || {
        let result = locked(&resolver).prefill_from_api(&key)?;
        Ok(WalkSummary { calls: result.calls, resolved: result.items.len() })
    })
    .await
}

#[tauri::command]
async fn preview(
    state: State<'_, AppState>,
    ids: Vec<String>,
    options: LaunchOptions,
) -> Result<LaunchPlan, String> {
    Ok(build_launch_plan(&state.paths, &ids, options))
}

#[tauri::command]
async fn launch(
    state: State<'_, AppState>,
    ids: Vec<String>,
    options: LaunchOptions,
) -> Result<Launched, String> {
    let plan = build_launch_plan(&state.paths, &ids, options);
    let session = Arc::clone(&state.session);
    let profiles = Arc::clone(&state.profiles);
    let workshop = state.paths.workshop.path.clone();
    blocking(move || {
        let launched = session.launch(&plan, &workshop).map_err(|error| error.to_string())?;
        // Recorded only once the game is actually running, so a rejected
        // launch leaves the previous record alone. The game is up either way,
        // so a failed write says so rather than reading as a failed launch.
        locked(&profiles)
            .record_launch(&ids, options)
            .map_err(|error| format!("launched, but the last launch was not recorded: {error}"))?;
        Ok(launched)
    })
    .await
}

#[tauri::command]
async fn list_profiles(state: State<'_, AppState>) -> Result<Profiles, String> {
    Ok(locked(&state.profiles).view())
}

#[tauri::command]
async fn save_profile(state: State<'_, AppState>, name: String) -> Result<Profiles, String> {
    let profiles = Arc::clone(&state.profiles);
    blocking(move || {
        let mut store = locked(&profiles);
        store.save_named(&name).map_err(|error| error.to_string())?;
        Ok(store.view())
    })
    .await
}

#[tauri::command]
async fn delete_profile(state: State<'_, AppState>, name: String) -> Result<Profiles, String> {
    let profiles = Arc::clone(&state.profiles);
    blocking(move || {
        let mut store = locked(&profiles);
        store.delete(&name).map_err(|error| error.to_string())?;
        Ok(store.view())
    })
    .await
}

#[tauri::command]
async fn stop(state: State<'_, AppState>) -> Result<(), String> {
    state.session.stop().map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            // The library knows nothing about Tauri; this closure is the only
            // place the two meet.
            let session = Arc::new(Session::new(Arc::new(move |event: Event| {
                let _ = handle.emit(EVENT, event);
            })));

            let paths = steam::discover();
            let resolver =
                Resolver::new(paths.workshop.path.clone(), config::cache_file());

            app.manage(AppState {
                resolver: Arc::new(Mutex::new(resolver)),
                session,
                paths: Arc::new(paths),
                profiles: Arc::new(Mutex::new(ProfileStore::new(config::profiles_file()))),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_mods,
            rescan,
            reset_cache,
            resolve_item,
            walk_all,
            preview,
            launch,
            stop,
            list_profiles,
            save_profile,
            delete_profile
        ])
        .run(tauri::generate_context!())
        .expect("brhap failed to start");
}
