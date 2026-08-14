//! Where brhap keeps its own files.
//!
//! The `~/.config/brhap/` directory and the `cache.json` path, with
//! `XDG_CONFIG_HOME` honored only when it is set and non-empty, since an
//! exported-but-empty value would otherwise resolve to the filesystem root.

use std::path::PathBuf;

/// Home directory, from `HOME`. Every supported platform sets it.
pub fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// Base config directory, honoring a set and non-empty `XDG_CONFIG_HOME`.
pub fn config_base() -> PathBuf {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => home_dir().join(".config"),
    }
}

/// Application name, and the directory it keeps its files in.
pub const APP_NAME: &str = "brhap";

/// Our own config directory. On macOS this is normally `~/.config/brhap`.
pub fn config_dir() -> PathBuf {
    config_base().join(APP_NAME)
}

/// The persisted dependency cache, shared with the Node implementation.
pub fn cache_file() -> PathBuf {
    config_dir().join("cache.json")
}

/// Saved launch profiles and the last launch. Ours alone; the Node
/// implementation has no profiles feature and never reads this file.
pub fn profiles_file() -> PathBuf {
    config_dir().join("profiles.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule that matters: empty is not the same as set. An exported but
    /// empty XDG_CONFIG_HOME must not resolve to the filesystem root.
    #[test]
    fn empty_xdg_value_falls_back_to_home() {
        temp_env(&[("HOME", Some("/home/tester")), ("XDG_CONFIG_HOME", Some(""))], || {
            assert_eq!(config_dir(), PathBuf::from("/home/tester/.config/brhap"));
        });
    }

    #[test]
    fn whitespace_xdg_value_falls_back_to_home() {
        temp_env(&[("HOME", Some("/home/tester")), ("XDG_CONFIG_HOME", Some("   "))], || {
            assert_eq!(config_dir(), PathBuf::from("/home/tester/.config/brhap"));
        });
    }

    #[test]
    fn unset_xdg_value_falls_back_to_home() {
        temp_env(&[("HOME", Some("/home/tester")), ("XDG_CONFIG_HOME", None)], || {
            assert_eq!(config_dir(), PathBuf::from("/home/tester/.config/brhap"));
        });
    }

    #[test]
    fn set_xdg_value_wins() {
        temp_env(&[("HOME", Some("/home/tester")), ("XDG_CONFIG_HOME", Some("/elsewhere"))], || {
            assert_eq!(cache_file(), PathBuf::from("/elsewhere/brhap/cache.json"));
        });
    }

    /// Environment variables are process wide, so these tests run one at a
    /// time behind a mutex and restore whatever was there before.
    fn temp_env(vars: &[(&str, Option<&str>)], body: impl FnOnce()) {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let guard = LOCK.get_or_init(|| Mutex::new(()));
        let _held = guard.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let previous: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|(key, _)| ((*key).to_string(), std::env::var(key).ok()))
            .collect();

        for (key, value) in vars {
            match value {
                Some(value) => unsafe { std::env::set_var(key, value) },
                None => unsafe { std::env::remove_var(key) },
            }
        }

        body();

        for (key, value) in previous {
            match value {
                Some(value) => unsafe { std::env::set_var(&key, value) },
                None => unsafe { std::env::remove_var(&key) },
            }
        }
    }
}
