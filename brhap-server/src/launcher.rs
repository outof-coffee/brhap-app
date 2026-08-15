//! Carrying out a launch.
//!
//! Put the game directory into the state a plan describes, then spawn the game
//! as a direct child process.
//!
//! Only symlinks pointing into the workshop content directory are ours to
//! remove. Anything else in the game directory is left alone, since this
//! writes inside the user's Steam install.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use crate::launch::LaunchPlan;

/// What applying a plan changed on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    pub removed: Vec<PathBuf>,
    pub created: Vec<PathBuf>,
}

/// Only links pointing into the workshop content directory are ours to
/// remove. A path that merely shares the prefix string is not a match.
pub fn is_managed_link(target: &Path, workshop_dir: &Path) -> bool {
    target.starts_with(workshop_dir) && target != workshop_dir
}

/// Remove the symlinks a previous launch created. Returns what it removed.
pub fn clear_managed_links(game_dir: &Path, workshop_dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    let entries = match std::fs::read_dir(game_dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(removed),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.file_type().is_symlink() {
            continue;
        }
        let Ok(target) = std::fs::read_link(&path) else {
            continue;
        };
        if !is_managed_link(&target, workshop_dir) {
            continue;
        }
        std::fs::remove_file(&path)?;
        removed.push(path);
    }
    Ok(removed)
}

/// Put the game directory into the state the plan describes: our old links
/// gone, one link per selected mod named for its workshop id.
///
/// A symlink already sitting at a link's own path is replaced outright: an
/// override target lives outside `workshop_dir`, so `clear_managed_links`
/// cannot have swept it, and a stale link there would otherwise make the
/// create below fail. Only a path already proven to be a symlink is touched.
pub fn apply_plan(plan: &LaunchPlan, workshop_dir: &Path) -> std::io::Result<Applied> {
    let removed = clear_managed_links(&plan.cwd, workshop_dir)?;
    let mut created = Vec::new();
    for link in &plan.symlinks {
        if let Ok(metadata) = std::fs::symlink_metadata(&link.link) {
            if metadata.file_type().is_symlink() {
                std::fs::remove_file(&link.link)?;
            }
        }
        std::os::unix::fs::symlink(&link.target, &link.link)?;
        created.push(link.link.clone());
    }
    Ok(Applied { removed, created })
}

/// Spawn the game as a direct child, with the working directory set to the
/// game folder and the Steam overlay injected. Not a Steam handoff, so the
/// returned handle is a real process we can watch and stop.
pub fn spawn_game(plan: &LaunchPlan) -> std::io::Result<Child> {
    let mut command = Command::new(&plan.executable);
    command.current_dir(&plan.cwd).args(&plan.args);
    for (key, value) in &plan.env {
        command.env(key, value);
    }
    command.spawn()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::Symlink;

    fn workshop() -> PathBuf {
        PathBuf::from("/steam/steamapps/workshop/content/107410")
    }

    #[test]
    fn accepts_links_into_the_workshop_content_directory() {
        assert!(is_managed_link(&workshop().join("450814997"), &workshop()));
    }

    #[test]
    fn rejects_links_elsewhere_in_the_game_directory() {
        assert!(!is_managed_link(Path::new("/Users/someone/mods/@custom"), &workshop()));
        assert!(!is_managed_link(Path::new("/usr/local/lib/whatever"), &workshop()));
    }

    #[test]
    fn rejects_a_path_that_only_shares_the_prefix_string() {
        let sibling = PathBuf::from("/steam/steamapps/workshop/content/107410-elsewhere/450814997");
        assert!(!is_managed_link(&sibling, &workshop()));
    }

    #[test]
    fn rejects_the_workshop_directory_itself() {
        assert!(!is_managed_link(&workshop(), &workshop()));
    }

    #[test]
    fn a_missing_game_directory_removes_nothing() {
        let removed = clear_managed_links(Path::new("/nonexistent/game"), &workshop()).expect("ok");
        assert!(removed.is_empty());
    }

    /// An override's target lives outside `workshop_dir`, so `clear_managed_links`
    /// never sweeps a stale one. Reapplying with a new target for the same
    /// link must still succeed and end up pointing at the new target.
    #[test]
    fn reapplying_a_plan_replaces_a_stale_link_pointing_elsewhere() {
        let game_dir = std::env::temp_dir().join(format!("brhap-test-reapply-{}", std::process::id()));
        let target_a = game_dir.join("target-a");
        let target_b = game_dir.join("target-b");
        std::fs::create_dir_all(&target_a).expect("make target a");
        std::fs::create_dir_all(&target_b).expect("make target b");
        let link = game_dir.join("450814997");

        let plan_a = LaunchPlan {
            executable: PathBuf::new(),
            cwd: game_dir.clone(),
            args: Vec::new(),
            env: Default::default(),
            symlinks: vec![Symlink { target: target_a.clone(), link: link.clone() }],
            preview: String::new(),
        };
        apply_plan(&plan_a, &workshop()).expect("first apply");
        assert_eq!(std::fs::read_link(&link).expect("read link"), target_a);

        let plan_b =
            LaunchPlan { symlinks: vec![Symlink { target: target_b.clone(), link: link.clone() }], ..plan_a };
        apply_plan(&plan_b, &workshop()).expect("second apply");
        assert_eq!(std::fs::read_link(&link).expect("read link"), target_b);

        std::fs::remove_dir_all(&game_dir).ok();
    }
}
