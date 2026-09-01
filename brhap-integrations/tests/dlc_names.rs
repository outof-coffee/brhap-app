//! Looks up the Arma 3 DLC ids from the local Steam environment (appinfo.vdf)
//! and resolves each to a name via brhap_server::store::dlc_names.

use brhap_server::steam::{arma_appinfo_entry, discover, listofdlc};
use brhap_server::store::dlc_names;

#[test]
fn prints_arma_3_dlc_names() {
    let paths = discover();
    if !paths.game.verified {
        println!("skipping: Arma 3 not found on this machine's Steam install");
        return;
    }

    let entry = arma_appinfo_entry(&paths.steam_dir).expect("entry present");
    let ids = listofdlc(&entry).expect("listofdlc present and parseable");

    let available = dlc_names(&ids).expect("dlc name lookup succeeded");
    for (id, name) in &available.names {
        match name {
            Some(name) => println!("{id}: {name}"),
            None => println!("{id}: <no name available>"),
        }
    }

    assert!(available.names.values().any(Option::is_some), "expected at least one resolved name");
}
