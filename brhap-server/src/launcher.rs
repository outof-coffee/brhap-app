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
pub fn apply_plan(plan: &LaunchPlan, workshop_dir: &Path) -> std::io::Result<Applied> {
    let removed = clear_managed_links(&plan.cwd, workshop_dir)?;
    let mut created = Vec::new();
    for link in &plan.symlinks {
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
}
