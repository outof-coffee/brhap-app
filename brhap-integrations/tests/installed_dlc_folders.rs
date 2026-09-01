//! Confirms installed_dlc_folders finds real, locally installed CDLC by
//! matching mod.cpp's appId against the ids from listofdlc.

use brhap_server::steam::{arma_appinfo_entry, discover, installed_dlc_folders, listofdlc};

#[test]
fn finds_locally_installed_cdlc_folders() {
    let paths = discover();
    if !paths.game.verified {
        println!("skipping: Arma 3 not found on this machine's Steam install");
        return;
    }

    let entry = arma_appinfo_entry(&paths.steam_dir).expect("entry present");
    let known_ids = listofdlc(&entry).expect("listofdlc present and parseable");

    let found = installed_dlc_folders(&paths.game.path, &known_ids);
    for folder in &found {
        println!("found installed dlc: {} in folder {}", folder.id, folder.folder_name);
    }
    assert!(
        found.iter().any(|folder| folder.id == 1681170),
        "expected Western Sahara (1681170) to be found"
    );
}
