use std::fmt;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
// implemented here because settings are separate from the profiles and server provided properties.
// in addition, settings are specific to the core application, not services it depends on.
use brhap_server::config::{config_dir};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub game_verified: bool,
    pub workshop_verified: bool,
    pub steam_keys: Vec<SteamApiKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamApiKey {
    pub steam_id: i64,
    pub steam_key: String,
}

/// Saved application settings.
pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load_settings(file: &Path) -> Settings {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(store: &Settings, file: &Path) -> Result<(), SettingsError> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(store)?;
    text.push('\n');
    match std::fs::write(file, text) {
        Ok(()) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

pub fn set_game_verified(store: &mut Settings, verified: bool) -> Result<(), SettingsError> {
    store.game_verified = verified;
    Ok(())
}

pub fn set_workshop_verified(store: &mut Settings, verified: bool) -> Result<(), SettingsError> {
    store.workshop_verified = verified;
    Ok(())
}

pub fn set_steam_key(store: &mut Settings, steam_id: i64, key: String) -> Result<(), SettingsError> {
    let index = store.steam_keys.iter().position(|k| k.steam_id == steam_id);
    match index {
        Some(i) => Ok(store.steam_keys[i].steam_key = key),
        None => Ok(store.steam_keys.push(SteamApiKey { steam_id, steam_key: key })),
    }
}

pub struct SettingsStore {
    path: PathBuf,
    data: Settings,
}

#[derive(Debug)]
pub enum SettingsError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl From<std::io::Error> for SettingsError {
    fn from(err: std::io::Error) -> Self {
        SettingsError::Io(err)
    }
}

impl From<serde_json::Error> for SettingsError {
    fn from(err: serde_json::Error) -> Self {
        SettingsError::Json(err)
    }
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::Io(err) => write!(f, "IO error: {}", err),
            SettingsError::Json(err) => write!(f, "JSON error: {}", err),
        }
    }
}

impl std::error::Error for SettingsError {}
impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        let data = load_settings(&path);
        Self { path, data }
    }

    pub fn view(&self) -> Settings { self.data.clone() }
    pub fn save_game_verified(&mut self, verified: bool) -> Result<(), SettingsError> {
        set_game_verified(&mut self.data, verified)?;
        self.save()
    }

    pub fn save_workshop_verified(&mut self, verified: bool) -> Result<(), SettingsError> {
        set_workshop_verified(&mut self.data, verified)?;
        self.save()
    }

    pub fn save_steam_key(&mut self, _steam_id: i64, key: String) -> Result<(), SettingsError> {
        // todo: implement steam_id info gathering
        set_steam_key(&mut self.data, 0, key)?;
        self.save()
    }

    pub fn save(&mut self) -> Result<(), SettingsError> {
        save_settings(&self.data, &self.path)
    }
}