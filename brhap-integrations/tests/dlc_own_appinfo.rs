//! Diagnostic: does a DLC's own appinfo.vdf entry (not Arma 3's) name its
//! install folder anywhere, the way the main game's installdir is read out
//! of its appmanifest? Checked against Prairie Fire (1227700), whose folder
//! is known to be `vn`, so a match there would confirm the idea.

use brhap_server::steam::discover;
use steam_vdf_parser::parse_appinfo;

const PRAIRIE_FIRE_ID: &str = "1227700";

#[test]
fn prints_prairie_fires_own_appinfo_entry() {
    let paths = discover();
    if !paths.game.verified {
        println!("skipping: Arma 3 not found on this machine's Steam install");
        return;
    }

    let appinfo_path = paths.steam_dir.join("appcache").join("appinfo.vdf");
    let bytes = std::fs::read(&appinfo_path).expect("appinfo.vdf readable");
    let vdf = parse_appinfo(&bytes).expect("appinfo.vdf parses");
    let root = vdf.as_obj().expect("root is an object");

    let Some(entry) = root.get(PRAIRIE_FIRE_ID) else {
        println!("no appinfo entry for {PRAIRIE_FIRE_ID} on this machine (not owned/cached?)");
        return;
    };

    let appinfo = entry.as_obj().and_then(|obj| obj.get("appinfo")).and_then(|v| v.as_obj());
    let Some(appinfo) = appinfo else {
        println!("entry has no appinfo object; entry = {entry:#?}");
        return;
    };

    println!("appinfo keys: {:?}", appinfo.iter().map(|(k, _)| k).collect::<Vec<_>>());

    if let Some(common) = appinfo.get("common").and_then(|v| v.as_obj()) {
        println!("common: {common:#?}");
    }
    if let Some(extended) = appinfo.get("extended").and_then(|v| v.as_obj()) {
        println!("extended: {extended:#?}");
    }
}
