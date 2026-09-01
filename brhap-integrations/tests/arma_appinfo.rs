//! Requires a real local Steam install with Arma 3 (107410) present.
//! Skips itself (prints and returns) when that isn't the case, rather than
//! failing, since this machine's Steam state isn't something the test can
//! control.

use brhap_server::steam::{arma_appinfo_entry, discover, listofdlc};

#[test]
fn finds_the_arma_3_appinfo_entry() {
    let paths = discover();
    if !paths.game.verified {
        println!("skipping: Arma 3 not found on this machine's Steam install");
        return;
    }

    let entry = arma_appinfo_entry(&paths.steam_dir);
    assert!(entry.is_some(), "expected an appinfo.vdf entry for app id 107410");
}

#[test]
fn finds_the_arma_3_dlc_list() {
    let paths = discover();
    if !paths.game.verified {
        println!("skipping: Arma 3 not found on this machine's Steam install");
        return;
    }

    let entry = arma_appinfo_entry(&paths.steam_dir).expect("entry present");
    let dlc = listofdlc(&entry).expect("listofdlc present and parseable");
    println!("dlc ids: {dlc:?}");
    assert!(!dlc.is_empty(), "expected at least one DLC id");
}
