//! brhap (Bohemian Rhapsody), the desktop wrapper.
//!
//! A Tauri v2 application that hosts the built web UI from ../web/dist. All of
//! the actual behaviour lives in ../brhap-core, which owns the operation set
//! every brhap frontend shares. There is no HTTP layer here: the commands below
//! are the desktop transport, and Tauri events replace the SSE stream.
//!
//! Everything this file adds is transport. The commands are one-line delegates,
//! and the only place Tauri and the core meet is the listener closure in
//! `main`. Core is blocking on purpose, so every command hands its work to
//! `spawn_blocking` rather than making the core async.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use brhap_core::{
    Core, Event, LaunchOptions, LaunchPlan, Launched, Overrides, Profiles, Resolved, Snapshot,
    WalkSummary,
};
use tauri::{Emitter, Manager, State};

/// Name the webview listens on for session events.
const EVENT: &str = "session-event";

struct AppState {
    core: Arc<Core>,
}

/// Run blocking core work off the UI thread and flatten the join error.
async fn blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work).await.map_err(|error| error.to_string())?
}

#[tauri::command]
async fn list_mods(state: State<'_, AppState>) -> Result<Snapshot, String> {
    Ok(state.core.snapshot())
}

#[tauri::command]
async fn rescan(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let core = Arc::clone(&state.core);
    blocking(move || Ok(core.rescan())).await
}

#[tauri::command]
async fn reset_cache(state: State<'_, AppState>) -> Result<Snapshot, String> {
    let core = Arc::clone(&state.core);
    blocking(move || Ok(core.reset_cache())).await
}

#[tauri::command]
async fn resolve_item(
    state: State<'_, AppState>,
    id: String,
    refresh: bool,
) -> Result<Resolved, String> {
    let core = Arc::clone(&state.core);
    blocking(move || core.resolve_item(&id, refresh)).await
}

#[tauri::command]
async fn walk_all(state: State<'_, AppState>) -> Result<WalkSummary, String> {
    let core = Arc::clone(&state.core);
    blocking(move || core.walk_all()).await
}

#[tauri::command]
async fn preview(
    state: State<'_, AppState>,
    ids: Vec<String>,
    options: LaunchOptions,
    overrides: Overrides,
) -> Result<LaunchPlan, String> {
    Ok(state.core.preview(&ids, options, &overrides))
}

#[tauri::command]
async fn launch(
    state: State<'_, AppState>,
    ids: Vec<String>,
    options: LaunchOptions,
    overrides: Overrides,
) -> Result<Launched, String> {
    let core = Arc::clone(&state.core);
    blocking(move || core.launch(&ids, options, &overrides)).await
}

#[tauri::command]
async fn stop(state: State<'_, AppState>) -> Result<(), String> {
    state.core.stop()
}

#[tauri::command]
async fn list_profiles(state: State<'_, AppState>) -> Result<Profiles, String> {
    Ok(state.core.list_profiles())
}

#[tauri::command]
async fn save_profile(state: State<'_, AppState>, name: String) -> Result<Profiles, String> {
    let core = Arc::clone(&state.core);
    blocking(move || core.save_profile(&name)).await
}

#[tauri::command]
async fn delete_profile(state: State<'_, AppState>, name: String) -> Result<Profiles, String> {
    let core = Arc::clone(&state.core);
    blocking(move || core.delete_profile(&name)).await
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            // The core knows nothing about Tauri; this closure is the only
            // place the two meet.
            let core = Core::new(Arc::new(move |event: Event| {
                let _ = handle.emit(EVENT, event);
            }));

            app.manage(AppState { core: Arc::new(core) });
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
