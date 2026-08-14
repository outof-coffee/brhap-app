//! Describing a launch without performing one.
//!
//! Build the executable path, working directory, argv, and environment from
//! the discovered Steam paths and the selected mod ids.
//!
//! The `-mod=` value is a single argument of bare workshop ids joined by
//! semicolons, with no quoting. Bare ids work because each mod is symlinked
//! into the game directory and the process runs with that directory as its
//! working directory. There is no shell involved, so literal quotes would end
//! up inside the value rather than being stripped.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ARMA_APP_ID;
use crate::steam::SteamPaths;

/// Startup parameters the UI can toggle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LaunchOptions {
    pub no_splash: bool,
    pub skip_intro: bool,
    pub empty_world: bool,
}

/// One link to create inside the game directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Symlink {
    /// Workshop directory being linked.
    pub target: PathBuf,
    /// Link inside the game directory, named for the workshop id.
    pub link: PathBuf,
}

/// Exactly what a launch would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlan {
    pub executable: PathBuf,
    /// Relative -mod= entries resolve against this, so it is the game dir.
    pub cwd: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub symlinks: Vec<Symlink>,
    /// A shell-quoted rendering of the same thing, for display only.
    pub preview: String,
}

/// The game binary inside the macOS app bundle, or the bare binary elsewhere.
///
/// The macOS path is confirmed on a real install. The Linux name is not: it
/// has never been run here, and needs checking on an actual Linux box before
/// it is trusted.
pub fn executable_for(game_dir: &std::path::Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        game_dir.join("ArmA3 AS Native.app").join("Contents").join("MacOS").join("ArmA3 AS Native")
    } else {
        game_dir.join("arma3_x64")
    }
}

/// Injecting this is what lets the Steam overlay attach to a direct spawn.
pub fn overlay_dylib(steam_dir: &std::path::Path) -> PathBuf {
    steam_dir.join("Steam.AppBundle").join("Steam").join("Contents").join("MacOS").join("gameoverlayrenderer.dylib")
}

fn needs_quoting(value: &str) -> bool {
    !value
        .chars()
        .all(|character| character.is_alphanumeric() || "@%+=:,./-_".contains(character))
}

fn quote(value: &str) -> String {
    if needs_quoting(value) {
        format!("'{}'", value.replace('\'', r"'\''"))
    } else {
        value.to_string()
    }
}

/// Describe a launch. Performs nothing.
pub fn build_launch_plan(paths: &SteamPaths, ids: &[String], options: LaunchOptions) -> LaunchPlan {
    let mut unique: Vec<String> = Vec::new();
    for id in ids {
        if !unique.contains(id) {
            unique.push(id.clone());
        }
    }

    let mut args: Vec<String> = Vec::new();
    if !unique.is_empty() {
        args.push(format!("-mod={}", unique.join(";")));
    }
    if options.no_splash {
        args.push("-noSplash".into());
    }
    if options.skip_intro {
        args.push("-skipIntro".into());
    }
    if options.empty_world {
        args.push("-world=empty".into());
    }

    let mut env = BTreeMap::new();
    env.insert("SteamAppId".to_string(), ARMA_APP_ID.to_string());
    if cfg!(target_os = "macos") {
        env.insert(
            "DYLD_INSERT_LIBRARIES".to_string(),
            overlay_dylib(&paths.steam_dir).to_string_lossy().to_string(),
        );
        env.insert("DYLD_FORCE_FLAT_NAMESPACE".to_string(), "1".to_string());
    }

    let cwd = paths.game.path.clone();
    let executable = executable_for(&cwd);
    let symlinks: Vec<Symlink> = unique
        .iter()
        .map(|id| Symlink {
            target: paths.workshop.path.join(id),
            link: cwd.join(id),
        })
        .collect();

    let env_text = env
        .iter()
        .map(|(key, value)| format!("{key}={}", quote(value)))
        .collect::<Vec<_>>()
        .join(" ");
    let args_text = args.iter().map(|arg| quote(arg)).collect::<Vec<_>>().join(" ");
    let preview = format!(
        "cd {} && {} {} {}",
        quote(&cwd.to_string_lossy()),
        env_text,
        quote(&executable.to_string_lossy()),
        args_text
    )
    .trim_end()
    .to_string();

    LaunchPlan { executable, cwd, args, env, symlinks, preview }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam::Located;

    fn paths() -> SteamPaths {
        SteamPaths {
            steam_dir: PathBuf::from("/steam"),
            game: Located { path: PathBuf::from("/steam/steamapps/common/Arma 3"), verified: true },
            workshop: Located {
                path: PathBuf::from("/steam/steamapps/workshop/content/107410"),
                verified: true,
            },
        }
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn joins_mod_ids_with_semicolons_in_one_argument() {
        let plan = build_launch_plan(&paths(), &ids(&["450814997", "463939057"]), LaunchOptions::default());
        assert_eq!(plan.args, vec!["-mod=450814997;463939057"]);
    }

    #[test]
    fn adds_no_mod_argument_when_nothing_is_selected() {
        let plan = build_launch_plan(&paths(), &[], LaunchOptions::default());
        assert!(plan.args.is_empty());
    }

    #[test]
    fn deduplicates_ids_without_reordering_them() {
        let plan = build_launch_plan(
            &paths(),
            &ids(&["463939057", "450814997", "463939057"]),
            LaunchOptions::default(),
        );
        assert_eq!(plan.args, vec!["-mod=463939057;450814997"]);
    }

    #[test]
    fn appends_only_the_options_that_are_enabled() {
        let options = LaunchOptions { no_splash: true, empty_world: true, skip_intro: false };
        let plan = build_launch_plan(&paths(), &ids(&["450814997"]), options);
        assert_eq!(plan.args, vec!["-mod=450814997", "-noSplash", "-world=empty"]);
    }

    #[test]
    fn runs_from_the_game_directory_so_bare_ids_resolve() {
        let plan = build_launch_plan(&paths(), &ids(&["450814997"]), LaunchOptions::default());
        assert_eq!(plan.cwd, PathBuf::from("/steam/steamapps/common/Arma 3"));
    }

    #[test]
    fn links_each_workshop_directory_in_under_its_id() {
        let plan = build_launch_plan(&paths(), &ids(&["450814997"]), LaunchOptions::default());
        assert_eq!(
            plan.symlinks,
            vec![Symlink {
                target: PathBuf::from("/steam/steamapps/workshop/content/107410/450814997"),
                link: PathBuf::from("/steam/steamapps/common/Arma 3/450814997"),
            }]
        );
    }

    #[test]
    fn never_puts_quotes_inside_the_mod_argument_itself() {
        let plan = build_launch_plan(&paths(), &ids(&["450814997", "463939057"]), LaunchOptions::default());
        assert!(!plan.args[0].contains('"'));
        assert!(!plan.args[0].contains('\''));
    }

    #[test]
    fn quotes_the_game_path_in_the_display_preview_only() {
        let plan = build_launch_plan(&paths(), &ids(&["450814997"]), LaunchOptions::default());
        assert!(plan.preview.contains("'/steam/steamapps/common/Arma 3'"));
        assert!(plan.preview.contains("-mod=450814997"));
    }
}
