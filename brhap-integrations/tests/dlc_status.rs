//! End-to-end: installed DLC rides through Resolver exactly like a mod,
//! against this machine's real Arma 3 install, with zero network calls.

use brhap_server::resolve::{ItemKind, Resolver};
use brhap_server::steam::discover;

#[test]
fn resolver_finds_installed_dlc_the_same_way_it_finds_mods() {
    let paths = discover();
    if !paths.game.verified {
        println!("skipping: Arma 3 not found on this machine's Steam install");
        return;
    }

    let resolver = Resolver::new(
        paths.workshop.path.clone(),
        std::env::temp_dir().join("brhap-integrations-dlc-status-cache.json"),
        paths.clone(),
    );

    for item in resolver.mods() {
        let view = resolver.view(&item.id);
        println!("{}: {} [{:?}] folder={:?}", view.id, item.name, view.kind, view.folder_name);
    }

    let western_sahara = resolver.view("1681170");
    assert!(western_sahara.installed, "expected Western Sahara to be marked installed");
    assert_eq!(western_sahara.kind, ItemKind::Cdlc);
    assert_eq!(western_sahara.folder_name.as_deref(), Some("WS"));

    let contact = resolver.view("1021790");
    assert!(contact.installed, "expected Contact to be marked installed");
    assert_eq!(contact.kind, ItemKind::Contact);
    assert_eq!(contact.folder_name.as_deref(), Some("Contact"));
}
