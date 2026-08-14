//! Finding the Steam install, the game, and the workshop content.
//!
//! Two-step resolution: the default Steam directory is the fallback guess,
//! `steamapps/libraryfolders.vdf` supplies further candidates, and each
//! candidate is confirmed by the presence of
//! `steamapps/appmanifest_107410.acf`. Confirmed beats guessed. An
//! unconfirmed guess is returned marked unverified rather than failing, since
//! the vdf's own `apps` index has been observed to be stale: on a real
//! machine it listed Arma 3 under an unmounted external library while the
//! actual install sat in the default location.
//!
//! Workshop content resolves independently against
//! `steamapps/workshop/content/107410`, because the two can live in different
//! libraries.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use crate::ARMA_APP_ID;
use crate::config::home_dir;

/// A resolved path, and whether anything on disk actually confirmed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub path: PathBuf,
    /// False means this is the fallback guess, not a confirmed location.
    pub verified: bool,
}

/// Where the game and the workshop content live. Resolved independently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamPaths {
    /// The Steam install itself, which is where the overlay library lives.
    pub steam_dir: PathBuf,
    pub game: Located,
    pub workshop: Located,
}

/// Default Steam directory for this platform.
pub fn default_steam_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        home_dir().join("Library/Application Support/Steam")
    } else {
        home_dir().join(".local/share/Steam")
    }
}

fn value_regex(key: &str, cell: &'static OnceLock<Regex>) -> &'static Regex {
    cell.get_or_init(|| Regex::new(&format!(r#""{key}"\s+"([^"]+)""#)).expect("static pattern"))
}

/// Every `path` entry in a libraryfolders.vdf, in file order, deduplicated.
/// The `apps` index inside the file is deliberately ignored.
pub fn parse_library_paths(vdf: &str) -> Vec<PathBuf> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let mut paths: Vec<PathBuf> = Vec::new();
    for capture in value_regex("path", &RE).captures_iter(vdf) {
        let path = PathBuf::from(&capture[1]);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

/// The `installdir` recorded in an appmanifest, which names the folder under
/// `steamapps/common` rather than assuming it is called "Arma 3".
pub fn parse_install_dir(manifest: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    value_regex("installdir", &RE)
        .captures(manifest)
        .map(|capture| capture[1].to_string())
}

fn manifest_path(library: &Path) -> PathBuf {
    library.join("steamapps").join(format!("appmanifest_{ARMA_APP_ID}.acf"))
}

fn workshop_path(library: &Path) -> PathBuf {
    library.join("steamapps").join("workshop").join("content").join(ARMA_APP_ID)
}

/// Candidate libraries: the default directory first, then whatever the vdf
/// lists. The default is included even when the vdf omits it.
pub fn library_candidates(steam_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![steam_dir.to_path_buf()];
    let vdf = steam_dir.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(&vdf) {
        for path in parse_library_paths(&text) {
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }
    }
    candidates
}

/// Resolve the game and workshop directories against a list of libraries,
/// falling back to a guess rooted at `steam_dir` when nothing confirms.
pub fn discover_in(candidates: &[PathBuf], steam_dir: &Path) -> SteamPaths {
    let mut game: Option<PathBuf> = None;
    let mut workshop: Option<PathBuf> = None;

    for library in candidates {
        if game.is_none()
            && let Ok(manifest) = std::fs::read_to_string(manifest_path(library))
        {
            let install_dir = parse_install_dir(&manifest).unwrap_or_else(|| "Arma 3".into());
            let path = library.join("steamapps").join("common").join(install_dir);
            if path.is_dir() {
                game = Some(path);
            }
        }
        if workshop.is_none() {
            let path = workshop_path(library);
            if path.is_dir() {
                workshop = Some(path);
            }
        }
    }

    SteamPaths {
        steam_dir: steam_dir.to_path_buf(),
        game: match game {
            Some(path) => Located { path, verified: true },
            None => Located {
                path: steam_dir.join("steamapps").join("common").join("Arma 3"),
                verified: false,
            },
        },
        workshop: match workshop {
            Some(path) => Located { path, verified: true },
            None => Located { path: workshop_path(steam_dir), verified: false },
        },
    }
}

/// Full discovery against this machine.
pub fn discover() -> SteamPaths {
    let steam_dir = default_steam_dir();
    discover_in(&library_candidates(&steam_dir), &steam_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VDF: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/Users/tester/Library/Application Support/Steam"
		"apps"
		{
			"8930"		"7418944313"
		}
	}
	"1"
	{
		"path"		"/Volumes/STICK/SteamLibrary"
		"apps"
		{
			"107410"		"92790810357"
		}
	}
}
"#;

    #[test]
    fn parses_every_library_path_in_order() {
        assert_eq!(
            parse_library_paths(VDF),
            vec![
                PathBuf::from("/Users/tester/Library/Application Support/Steam"),
                PathBuf::from("/Volumes/STICK/SteamLibrary"),
            ]
        );
    }

    #[test]
    fn ignores_the_apps_index_even_when_it_claims_the_game() {
        // The vdf says 107410 lives on STICK. Only appmanifest on disk decides,
        // so parsing must not surface that claim as a path of its own.
        let paths = parse_library_paths(VDF);
        assert!(!paths.iter().any(|path| path.to_string_lossy().contains("107410")));
    }

    #[test]
    fn deduplicates_repeated_paths() {
        let vdf = r#""path" "/a" "path" "/a" "path" "/b""#;
        assert_eq!(parse_library_paths(vdf), vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    #[test]
    fn empty_or_unparsable_vdf_yields_nothing() {
        assert!(parse_library_paths("").is_empty());
        assert!(parse_library_paths("not a vdf at all").is_empty());
    }

    #[test]
    fn reads_the_install_dir_from_a_manifest() {
        let manifest = "\"AppState\"\n{\n\t\"appid\"\t\"107410\"\n\t\"installdir\"\t\"Arma 3\"\n}";
        assert_eq!(parse_install_dir(manifest).as_deref(), Some("Arma 3"));
    }

    #[test]
    fn missing_install_dir_is_none() {
        assert_eq!(parse_install_dir("\"AppState\" { \"appid\" \"107410\" }"), None);
    }

    #[test]
    fn falls_back_to_an_unverified_guess_when_nothing_confirms() {
        let steam = PathBuf::from("/nonexistent/Steam");
        let paths = discover_in(std::slice::from_ref(&steam), &steam);
        assert!(!paths.game.verified);
        assert!(!paths.workshop.verified);
        assert_eq!(paths.game.path, steam.join("steamapps/common/Arma 3"));
    }
}
