//! Checks whether the four appdetails-DLC ids that resolved no store name are
//! actually Workshop item ids instead, using the same scrape path production
//! code (resolve.rs) uses for real Workshop lookups.

use brhap_server::scrape::fetch_workshop_page;

const UNRESOLVED_DLC_IDS: [&str; 4] = ["249861", "249862", "304400", "612480"];

#[test]
fn checks_whether_unresolved_dlc_ids_are_workshop_items() {
    for id in UNRESOLVED_DLC_IDS {
        match fetch_workshop_page(id) {
            Ok(page) => println!("{id}: workshop item, name = {:?}", page.name),
            Err(error) => println!("{id}: not a workshop item ({error})"),
        }
    }
}
