//! Everything that can happen, in one place.

use std::path::PathBuf;

use brhap_core::{Launched, Profiles, Resolved, Settings, Snapshot};

use crate::events::Incoming;
use crate::state::{Flag, Screen};

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Loaded(Snapshot),
    Toggled(String, bool),
    Flagged(Flag, bool),
    Resolved(String, Result<Resolved, String>),
    TogglePreview,
    Launch,
    Launched(Result<Launched, String>),
    Stop,
    Stopped(Result<(), String>),
    /// Rescan the workshop directory, then walk the Steam API if a key is set.
    Refresh,
    /// Re-resolve every selected mod, staggered to stay clear of rate limiting.
    Refetch,
    ResetCache,
    ResetConfirmed(bool),
    /// A rescan replaces what is known rather than adding to it, so it is kept
    /// apart from `Loaded`. Carries the note to show alongside it.
    Rescanned(Snapshot, String),
    PickOverride(String),
    OverridePicked(String, Option<PathBuf>),
    ClearOverride(String),
    Show(Screen),
    ProfileName(String),
    SaveProfile,
    DeleteProfile(String),
    ApplyProfile(String),
    /// List and delete answer with the whole store, so the caller restates
    /// rather than patches.
    Stored(Result<Profiles, String>),
    /// Save answers the same way, but is kept separate because only a save
    /// that went through should clear the name field.
    Saved(Result<Profiles, String>),
    /// Open the key editor.
    EditSteamKey,
    KeyInput(String),
    /// Show the key being typed rather than masking it.
    ToggleReveal,
    CommitSteamKey,
    /// Forget the saved key, leaving whatever the environment holds.
    ClearSteamKey,
    CancelKeyEdit,
    SettingsLoaded(Settings),
    SettingsSaved(Result<Settings, String>),
    Session(Incoming),
}
