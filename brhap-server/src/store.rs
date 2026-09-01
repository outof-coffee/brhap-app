//! Resolving DLC names via the Store's public appdetails endpoint.
//!
//! No API key needed for this lookup, unlike the official partner Web API in
//! api.rs: this is the same endpoint the store website itself uses.

use std::collections::HashMap;

use serde_json::Value;

const ENDPOINT: &str = "https://store.steampowered.com/api/appdetails";

/// App ids mapped to their Store name, or None when the id has no store page.
/// Not every id in a DLC list resolves one; some are unrelated ids.
pub struct AvailableDlc {
    pub names: HashMap<u64, Option<String>>,
}

/// Pull the name out of one appdetails response body for the id it was
/// fetched for. A missing or unsuccessful entry yields None rather than an
/// error, since that is Steam's own signal that the id has no store page.
fn parse_name(body: &Value, id: u64) -> Option<String> {
    body.get(id.to_string())
        .filter(|entry| entry.get("success").and_then(Value::as_bool) == Some(true))
        .and_then(|entry| entry.get("data"))
        .and_then(|data| data.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn fetch_name(client: &reqwest::blocking::Client, id: u64) -> Result<Option<String>, String> {
    let url = format!("{ENDPOINT}?appids={id}");
    let response = client.get(&url).send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Steam Store API returned HTTP {}", response.status().as_u16()));
    }
    let body = response.text().map_err(|error| error.to_string())?;
    let parsed: Value = serde_json::from_str(&body).map_err(|error| error.to_string())?;
    Ok(parse_name(&parsed, id))
}

/// Resolve a Store name for each given app id, one request per id.
pub fn dlc_names(ids: &[u64]) -> Result<AvailableDlc, String> {
    let client = reqwest::blocking::Client::new();
    let mut names = HashMap::new();
    for &id in ids {
        names.insert(id, fetch_name(&client, id)?);
    }
    Ok(AvailableDlc { names })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_name_from_a_successful_entry() {
        let body = json!({ "395180": { "success": true, "data": { "name": "Arma 3 Apex" } } });
        assert_eq!(parse_name(&body, 395180), Some("Arma 3 Apex".to_string()));
    }

    #[test]
    fn an_unsuccessful_entry_yields_none() {
        let body = json!({ "249861": { "success": false } });
        assert_eq!(parse_name(&body, 249861), None);
    }

    #[test]
    fn a_missing_entry_yields_none() {
        assert_eq!(parse_name(&json!({}), 1), None);
    }
}
