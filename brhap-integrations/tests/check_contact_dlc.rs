use brhap_server::steam::{check_contact_dlc, discover};

#[test]
fn checks_whether_the_contact_folder_has_anything_in_it() {
    let paths = discover();
    if !paths.game.verified {
        println!("skipping: Arma 3 not found on this machine's Steam install");
        return;
    }

    println!("contact folder: {:?}", check_contact_dlc(&paths.game.path));
}
