//! Terminal entry point for brhap-server.
//!
//! Deliberately thin. Its only job is to exercise the library without Tauri,
//! so each phase of the port can be checked from a shell as it lands.

use std::collections::{BTreeMap, HashSet};

use brhap_server::{ARMA_APP_ID, api, cache, config, launch, mods, resolve, steam};

fn main() {
    // `cargo run -- fetch <id>` dumps what Steam actually returns for one
    // workshop page, which is the only way to tell a stripped page from a
    // parser bug.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("fetch") {
        let Some(id) = args.get(2) else {
            eprintln!("usage: rarma-server fetch <workshop id>");
            std::process::exit(2);
        };
        dump_page(id);
        return;
    }

    println!("brhap-server {} (app id {})", env!("CARGO_PKG_VERSION"), ARMA_APP_ID);
    println!("config dir:  {}", config::config_dir().display());
    println!("cache file:  {}", config::cache_file().display());

    let steam_dir = steam::default_steam_dir();
    println!("steam guess: {}", steam_dir.display());
    for (index, library) in steam::library_candidates(&steam_dir).iter().enumerate() {
        println!("  library {index}: {}", library.display());
    }

    let paths = steam::discover();
    let mark = |located: &steam::Located| if located.verified { "confirmed" } else { "UNVERIFIED guess" };
    println!("game:        {} [{}]", paths.game.path.display(), mark(&paths.game));
    println!("workshop:    {} [{}]", paths.workshop.path.display(), mark(&paths.workshop));

    let installed = mods::list_mods(&paths.workshop.path);
    println!("installed mods: {}", installed.len());
    for item in &installed {
        println!("  {:<11} {}", item.id, item.name);
    }

    // Read only. Nothing here writes the cache the Node server also uses, and
    // nothing here fetches: `view` is the no-network half of the protocol.
    let ids: HashSet<String> = installed.iter().map(|item| item.id.clone()).collect();
    let cached = cache::load_cache(&ids, &config::cache_file());
    let (entries, installed_deps): (usize, usize) = cached
        .ids()
        .fold((0, 0), |(entries, installed_deps), id| match cached.status(id) {
            Some(cache::CacheStatus::NotInstalled(_)) => (entries + 1, installed_deps),
            Some(cache::CacheStatus::Installed(_)) => (entries, installed_deps + 1),
            None => (entries, installed_deps),
        });
    println!("cache: {entries} entry(s), {installed_deps} installed dep list(s)");

    let resolver = resolve::Resolver::new(paths.workshop.path.clone(), config::cache_file(), paths.clone());
    println!("resolver views:");
    for item in resolver.mods() {
        let view = resolver.view(&item.id);
        println!(
            "  {:<11} {:<20} source {:?} requires {:?}",
            view.id,
            view.name.unwrap_or_default(),
            view.source,
            view.requires.unwrap_or_default()
        );
    }
    let unknown = resolver.view("999999999");
    println!("  unknown id -> name {:?} source {:?}", unknown.name, unknown.source);

    // Presence only. The key itself is never printed.
    println!(
        "steam api key ({}): {}",
        api::KEY_VAR,
        if api::load_steam_key().is_some() { "present" } else { "absent" }
    );

    // Describes a launch of every installed mod. Creates no symlinks and
    // spawns nothing; this is the plan, not the act.
    let selected: Vec<String> = installed.iter().map(|item| item.id.clone()).collect();
    let plan =
        launch::build_launch_plan(&paths, &selected, &[], launch::LaunchOptions::default(), &BTreeMap::new());
    println!("launch plan: {} symlink(s) would be created", plan.symlinks.len());
    println!("{}", plan.preview);
}

/// Fetch one workshop page with the tool's real user agent and report what
/// came back: status, size, and whether the markers we parse are present.
fn dump_page(id: &str) {
    // A full URL is used verbatim, so this can be pointed at a local capture
    // server to see exactly which headers ureq sends.
    let url = if id.starts_with("http") {
        id.to_string()
    } else {
        format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}")
    };
    println!("GET {url}");
    println!("user-agent: {}", brhap_server::scrape::USER_AGENT);

    let client = match brhap_server::scrape::client() {
        Ok(client) => client,
        Err(error) => {
            println!("client build failed: {error}");
            return;
        }
    };

    let response = match client.get(&url).send() {
        Ok(response) => response,
        Err(error) => {
            println!("request failed: {error}");
            return;
        }
    };

    println!("status: {}", response.status());
    let body = match response.text() {
        Ok(body) => body,
        Err(error) => {
            println!("body unreadable: {error}");
            return;
        }
    };

    println!("bytes: {}", body.len());
    println!("has workshopItemTitle: {}", body.contains("workshopItemTitle"));
    println!("has RequiredItems: {}", body.contains("RequiredItems"));
    println!("is_item_page: {}", brhap_server::scrape::is_item_page(&body));
    println!("--- first 400 chars ---");
    println!("{}", body.chars().take(400).collect::<String>());
}
