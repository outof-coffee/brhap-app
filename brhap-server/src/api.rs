//! The optional Steam Web API path.
//!
//! When a key is present, `IPublishedFileService/GetDetails` resolves the
//! whole tree in one batched call per level, filling the cache in a single
//! deliberate action. An addition to the click-driven scrape, not a
//! replacement, since the key is optional.
//!
//! The keyless `ISteamRemoteStorage` variant is not usable here: it returns no
//! `children` field at all.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

/// Stop runaway walks if the graph is deeper or more tangled than expected.
const MAX_ROUNDS: usize = 10;

const ENDPOINT: &str = "https://api.steampowered.com/IPublishedFileService/GetDetails/v1/";

/// One item as the API describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiItem {
    pub name: String,
    pub requires: Vec<String>,
}

/// Everything a walk reached, and what it cost.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalkResult {
    pub items: BTreeMap<String, ApiItem>,
    /// Number of API calls made, which equals the depth of the tree.
    pub calls: usize,
}

/// Environment variable holding the optional Steam Web API key.
pub const KEY_VAR: &str = "STEAM_KEY";

/// Read the key from the environment, and only from there.
///
/// No .env handling: a path baked in at compile time would not survive a
/// shipped desktop build. A real configuration file comes later.
pub fn load_steam_key() -> Option<String> {
    match std::env::var(KEY_VAR) {
        Ok(key) if !key.trim().is_empty() => Some(key),
        _ => None,
    }
}

/// Pull items and their children out of a GetDetails response body.
pub fn parse_details(body: &Value) -> BTreeMap<String, ApiItem> {
    let mut items = BTreeMap::new();
    let Some(details) = body.get("response").and_then(|value| value.get("publishedfiledetails"))
    else {
        return items;
    };
    let Some(details) = details.as_array() else {
        return items;
    };

    for detail in details {
        let Some(id) = detail.get("publishedfileid").and_then(Value::as_str) else {
            continue;
        };
        let requires = detail
            .get("children")
            .and_then(Value::as_array)
            .map(|children| {
                children
                    .iter()
                    .filter_map(|child| {
                        child.get("publishedfileid").and_then(Value::as_str).map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let name = detail
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string();
        items.insert(id.to_string(), ApiItem { name, requires });
    }
    items
}

fn fetch_details(ids: &[String], key: &str) -> Result<Value, String> {
    let mut url = reqwest::Url::parse(ENDPOINT).map_err(|error| error.to_string())?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("key", key);
        query.append_pair("includechildren", "true");
        for (index, id) in ids.iter().enumerate() {
            query.append_pair(&format!("publishedfileids[{index}]"), id);
        }
    }

    let client = crate::scrape::client().map_err(|error| error.to_string())?;
    let response = client.get(url).send().map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Steam API returned HTTP {}", response.status().as_u16()));
    }
    let body = response.text().map_err(|error| error.to_string())?;
    serde_json::from_str(&body).map_err(|error| error.to_string())
}

/// Walk the dependency tree breadth first, one batched call per level, so the
/// call count follows the depth of the tree rather than the number of items.
pub fn walk_dependencies(ids: &[String], key: &str) -> Result<WalkResult, String> {
    let mut result = WalkResult::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut frontier: Vec<String> = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            frontier.push(id.clone());
        }
    }
    seen.clear();

    let mut round = 0;
    while !frontier.is_empty() && round < MAX_ROUNDS {
        round += 1;
        for id in &frontier {
            seen.insert(id.clone());
        }

        let body = fetch_details(&frontier, key)?;
        result.calls += 1;

        let mut next: Vec<String> = Vec::new();
        for (id, item) in parse_details(&body) {
            for child in &item.requires {
                if !seen.contains(child) && !next.contains(child) {
                    next.push(child.clone());
                }
            }
            result.items.insert(id, item);
        }
        frontier = next;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "response": {
                "publishedfiledetails": [
                    { "publishedfileid": "450814997", "title": "CBA_A3", "num_children": 0 },
                    {
                        "publishedfileid": "2738615029",
                        "title": "Misery",
                        "num_children": 3,
                        "children": [
                            { "publishedfileid": "450814997" },
                            { "publishedfileid": "463939057" },
                            { "publishedfileid": "2888888564" }
                        ]
                    }
                ]
            }
        })
    }

    #[test]
    fn reads_titles_and_children() {
        let items = parse_details(&sample());
        assert_eq!(items.get("450814997").map(|item| item.name.clone()), Some("CBA_A3".into()));
        assert_eq!(
            items.get("2738615029").map(|item| item.requires.clone()),
            Some(vec!["450814997".into(), "463939057".into(), "2888888564".into()])
        );
    }

    /// An item with no requirements has no `children` key at all, rather than
    /// an empty array, so its absence must read as "none".
    #[test]
    fn a_missing_children_key_means_no_requirements() {
        let items = parse_details(&sample());
        assert_eq!(items.get("450814997").map(|item| item.requires.len()), Some(0));
    }

    #[test]
    fn an_unexpected_body_yields_nothing() {
        assert!(parse_details(&json!({})).is_empty());
        assert!(parse_details(&json!({ "response": { "publishedfiledetails": "nope" } })).is_empty());
    }
}
