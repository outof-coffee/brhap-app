//! Diagnostic: list short-named (<=4 char) folders directly under the Arma 3
//! game directory, and print any mod.cpp each one contains, to check the
//! proposed heuristic (short folder name + mod.cpp naming it "Creator DLC").

use brhap_server::steam::discover;

#[test]
fn prints_short_folders_and_their_mod_cpp() {
    let paths = discover();
    if !paths.game.verified {
        println!("skipping: Arma 3 not found on this machine's Steam install");
        return;
    }

    let entries = std::fs::read_dir(&paths.game.path).expect("game dir readable");
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if name.len() > 4 {
            continue;
        }

        let mod_cpp = path.join("mod.cpp");
        match std::fs::read_to_string(&mod_cpp) {
            Ok(contents) => println!("=== {name} ===\n{contents}\n"),
            Err(_) => println!("=== {name} === (no mod.cpp)"),
        }
    }
}
