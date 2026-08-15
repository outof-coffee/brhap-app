//! Saved launch profiles.
//!
//! A `profiles.json` holding two things: `lastLaunch`, overwritten every time a
//! launch actually succeeds, and `profiles`, the named copies the user chose to
//! keep. Naming a profile always copies the last launch, so the name is the
//! only thing the user types.
//!
//! The mutations are free functions over a `Profiles` so they can be tested
//! without touching disk, the same split `cache` uses between `merge_cache` and
//! `load_cache`. `ProfileStore` is the thin wrapper that owns a path and
//! persists after each change.
//!
//! Field names are camelCase because this file is handed to the frontend as is.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::launch::LaunchOptions;

/// What was selected the last time a launch succeeded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LastLaunch {
    pub ids: Vec<String>,
    pub options: LaunchOptions,
    pub overrides: BTreeMap<String, PathBuf>,
}

/// A named launch, kept until the user deletes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub ids: Vec<String>,
    pub options: LaunchOptions,
    #[serde(default)]
    pub overrides: BTreeMap<String, PathBuf>,
}

/// The whole file. A Vec rather than a map so the list keeps the order the
/// profiles were saved in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profiles {
    pub last_launch: Option<LastLaunch>,
    pub profiles: Vec<Profile>,
}

#[derive(Debug)]
pub enum ProfileError {
    /// Nothing has been launched yet, so there is nothing to name.
    NoLastLaunch,
    EmptyName,
    DuplicateName(String),
    UnknownName(String),
    Io(std::io::Error),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoLastLaunch => write!(formatter, "nothing has been launched yet"),
            Self::EmptyName => write!(formatter, "give the profile a name"),
            Self::DuplicateName(name) => write!(formatter, "a profile named \"{name}\" already exists"),
            Self::UnknownName(name) => write!(formatter, "no profile named \"{name}\""),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ProfileError {}

impl From<std::io::Error> for ProfileError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Record a successful launch, replacing whatever was there before.
pub fn record_launch(
    store: &mut Profiles,
    ids: &[String],
    options: LaunchOptions,
    overrides: &BTreeMap<String, PathBuf>,
) {
    store.last_launch = Some(LastLaunch { ids: ids.to_vec(), options, overrides: overrides.clone() });
}

/// Keep the last launch under a name. Names are unique, so a name already in
/// use is refused rather than silently replacing the profile behind it.
pub fn add_named(store: &mut Profiles, name: &str) -> Result<(), ProfileError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ProfileError::EmptyName);
    }
    if store.profiles.iter().any(|profile| profile.name == name) {
        return Err(ProfileError::DuplicateName(name.to_string()));
    }
    let last = store.last_launch.as_ref().ok_or(ProfileError::NoLastLaunch)?;

    store.profiles.push(Profile {
        name: name.to_string(),
        ids: last.ids.clone(),
        options: last.options,
        overrides: last.overrides.clone(),
    });
    Ok(())
}

/// Forget a saved profile. The last launch is untouched.
pub fn remove(store: &mut Profiles, name: &str) -> Result<(), ProfileError> {
    let before = store.profiles.len();
    store.profiles.retain(|profile| profile.name != name);
    if store.profiles.len() == before {
        return Err(ProfileError::UnknownName(name.to_string()));
    }
    Ok(())
}

/// Load the saved profiles. A missing or corrupt file is simply empty.
pub fn load_profiles(file: &Path) -> Profiles {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write the profiles back out, creating the config directory if needed.
pub fn save_profiles(store: &Profiles, file: &Path) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(store)?;
    text.push('\n');
    std::fs::write(file, text)
}

/// The profiles as held by a running app: a path and the data at it.
///
/// Unlike the dependency cache, a failed write here is reported rather than
/// swallowed. The cache can be rebuilt from Steam; a profile the user named
/// cannot be rebuilt from anything.
pub struct ProfileStore {
    path: PathBuf,
    data: Profiles,
}

impl ProfileStore {
    pub fn new(path: PathBuf) -> Self {
        let data = load_profiles(&path);
        Self { path, data }
    }

    /// A copy for the frontend.
    pub fn view(&self) -> Profiles {
        self.data.clone()
    }

    pub fn record_launch(
        &mut self,
        ids: &[String],
        options: LaunchOptions,
        overrides: &BTreeMap<String, PathBuf>,
    ) -> Result<(), ProfileError> {
        record_launch(&mut self.data, ids, options, overrides);
        self.save()
    }

    pub fn save_named(&mut self, name: &str) -> Result<(), ProfileError> {
        add_named(&mut self.data, name)?;
        self.save()
    }

    pub fn delete(&mut self, name: &str) -> Result<(), ProfileError> {
        remove(&mut self.data, name)?;
        self.save()
    }

    fn save(&self) -> Result<(), ProfileError> {
        save_profiles(&self.data, &self.path).map_err(ProfileError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    fn no_overrides() -> BTreeMap<String, PathBuf> {
        BTreeMap::new()
    }

    fn launched() -> Profiles {
        let mut store = Profiles::default();
        record_launch(
            &mut store,
            &ids(&["450814997", "463939057"]),
            LaunchOptions {
                no_splash: true,
                skip_intro: false,
                empty_world: false,
                intel_mode: false,
                steam_overlay: false,
            },
            &no_overrides(),
        );
        store
    }

    #[test]
    fn a_new_launch_replaces_the_previous_record() {
        let mut store = launched();
        record_launch(&mut store, &ids(&["111"]), LaunchOptions::default(), &no_overrides());
        assert_eq!(store.last_launch.map(|last| last.ids), Some(ids(&["111"])));
    }

    #[test]
    fn naming_copies_the_last_launch() {
        let mut store = launched();
        add_named(&mut store, "night ops").expect("saves");
        let profile = store.profiles.first().expect("one profile");
        assert_eq!(profile.name, "night ops");
        assert_eq!(profile.ids, ids(&["450814997", "463939057"]));
        assert!(profile.options.no_splash);
    }

    #[test]
    fn naming_copies_overrides_from_the_last_launch() {
        let mut store = Profiles::default();
        let mut overrides = BTreeMap::new();
        overrides.insert("450814997".to_string(), PathBuf::from("/Users/tester/mymod"));
        record_launch(&mut store, &ids(&["450814997"]), LaunchOptions::default(), &overrides);
        add_named(&mut store, "night ops").expect("saves");
        assert_eq!(store.profiles[0].overrides, overrides);
    }

    /// The saved copy is a copy. Launching something else afterwards must not
    /// rewrite a profile the user already named.
    #[test]
    fn a_saved_profile_does_not_follow_later_launches() {
        let mut store = launched();
        add_named(&mut store, "night ops").expect("saves");
        record_launch(&mut store, &ids(&["111"]), LaunchOptions::default(), &no_overrides());
        assert_eq!(store.profiles[0].ids, ids(&["450814997", "463939057"]));
    }

    #[test]
    fn a_name_is_trimmed_before_it_is_used() {
        let mut store = launched();
        add_named(&mut store, "  night ops  ").expect("saves");
        assert_eq!(store.profiles[0].name, "night ops");
    }

    #[test]
    fn refuses_a_blank_name() {
        let mut store = launched();
        assert!(matches!(add_named(&mut store, "   "), Err(ProfileError::EmptyName)));
        assert!(store.profiles.is_empty());
    }

    #[test]
    fn refuses_a_name_already_in_use() {
        let mut store = launched();
        add_named(&mut store, "night ops").expect("saves");
        assert!(matches!(
            add_named(&mut store, "night ops"),
            Err(ProfileError::DuplicateName(_))
        ));
        assert_eq!(store.profiles.len(), 1);
    }

    #[test]
    fn refuses_to_name_a_launch_that_never_happened() {
        let mut store = Profiles::default();
        assert!(matches!(add_named(&mut store, "night ops"), Err(ProfileError::NoLastLaunch)));
    }

    #[test]
    fn removing_drops_only_the_named_profile() {
        let mut store = launched();
        add_named(&mut store, "night ops").expect("saves");
        add_named(&mut store, "daytime").expect("saves");
        remove(&mut store, "night ops").expect("removes");
        assert_eq!(store.profiles.len(), 1);
        assert_eq!(store.profiles[0].name, "daytime");
        assert!(store.last_launch.is_some(), "deleting a profile leaves the last launch alone");
    }

    #[test]
    fn removing_an_unknown_name_is_an_error() {
        let mut store = launched();
        assert!(matches!(remove(&mut store, "nope"), Err(ProfileError::UnknownName(_))));
    }

    #[test]
    fn treats_a_missing_file_as_empty() {
        assert_eq!(load_profiles(Path::new("/nonexistent/profiles.json")), Profiles::default());
    }

    /// The frontend reads these keys directly, so the camelCase names have to
    /// survive a round trip.
    #[test]
    fn round_trips_through_the_file_shape() {
        let mut store = launched();
        store.last_launch.as_mut().expect("just launched").overrides =
            BTreeMap::from([("450814997".to_string(), PathBuf::from("/Users/tester/mymod"))]);
        add_named(&mut store, "night ops").expect("saves");
        store.profiles[0].overrides = store.last_launch.as_ref().expect("just launched").overrides.clone();

        let text = serde_json::to_string(&store).expect("serializes");
        assert!(text.contains("\"lastLaunch\""));
        assert!(text.contains("\"noSplash\""));
        assert!(text.contains("\"overrides\""));
        assert!(text.contains("\"450814997\":\"/Users/tester/mymod\""));

        let reparsed: Profiles = serde_json::from_str(&text).expect("parses");
        assert_eq!(reparsed, store);
    }

    /// A file written before a field existed still has to load, so every key
    /// is optional on the way in.
    #[test]
    fn loads_a_file_that_is_missing_sections() {
        let store: Profiles = serde_json::from_str("{}").expect("parses");
        assert_eq!(store, Profiles::default());
    }
}
