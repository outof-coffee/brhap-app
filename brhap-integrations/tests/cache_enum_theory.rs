//! Proves, with throwaway toy types and a throwaway temp file (never the
//! real cache.json), that one enum-tagged BTreeMap can be saved to and
//! loaded from disk in the same two-section ("entries"/"installedDeps")
//! shape the real cache.rs on-disk format uses, before touching real code.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotInstalled {
    name: String,
    requires: Option<Vec<String>>,
    fetched_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Installed {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    requires: Vec<String>,
    fetched_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Record {
    Installed(Installed),
    NotInstalled(NotInstalled),
}

/// One map in memory, split into the two on-disk sections on save.
fn save(records: &BTreeMap<String, Record>, file: &std::path::Path) -> std::io::Result<()> {
    let mut entries = BTreeMap::new();
    let mut installed_deps = BTreeMap::new();
    for (id, record) in records {
        match record {
            Record::NotInstalled(entry) => {
                entries.insert(id.clone(), entry.clone());
            }
            Record::Installed(deps) => {
                installed_deps.insert(id.clone(), deps.clone());
            }
        }
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Out<'a> {
        version: u32,
        entries: &'a BTreeMap<String, NotInstalled>,
        installed_deps: &'a BTreeMap<String, Installed>,
    }

    let text = serde_json::to_string_pretty(&Out { version: 2, entries: &entries, installed_deps: &installed_deps })?;
    std::fs::write(file, text)
}

/// The two on-disk sections merged back into one map.
fn load(file: &std::path::Path) -> BTreeMap<String, Record> {
    let mut records = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(file) else { return records };
    let Ok(raw): Result<Value, _> = serde_json::from_str(&text) else { return records };

    if let Some(entries) = raw.get("entries").and_then(Value::as_object) {
        for (id, value) in entries {
            if let Ok(entry) = serde_json::from_value::<NotInstalled>(value.clone()) {
                records.insert(id.clone(), Record::NotInstalled(entry));
            }
        }
    }
    if let Some(deps) = raw.get("installedDeps").and_then(Value::as_object) {
        for (id, value) in deps {
            if let Ok(entry) = serde_json::from_value::<Installed>(value.clone()) {
                records.insert(id.clone(), Record::Installed(entry));
            }
        }
    }
    records
}

#[test]
fn one_enum_tagged_map_saves_and_loads_the_two_section_shape() {
    let mut records = BTreeMap::new();
    records.insert(
        "450814997".to_string(),
        Record::NotInstalled(NotInstalled {
            name: "CBA_A3".to_string(),
            requires: None,
            fetched_at: String::new(),
        }),
    );
    records.insert(
        "463939057".to_string(),
        Record::Installed(Installed {
            name: Some("ace".to_string()),
            requires: vec!["450814997".to_string()],
            fetched_at: "2026-08-13T00:00:00.000Z".to_string(),
        }),
    );

    let file = std::env::temp_dir().join("brhap-integrations-cache-enum-theory.json");
    save(&records, &file).expect("saves");

    let text = std::fs::read_to_string(&file).expect("file written");
    println!("on-disk shape:\n{text}");
    assert!(text.contains("\"entries\""));
    assert!(text.contains("\"installedDeps\""));
    assert!(text.contains("\"450814997\""));
    assert!(text.contains("\"463939057\""));

    let loaded = load(&file);
    assert_eq!(loaded, records, "round trip through disk must reproduce the same records");

    std::fs::remove_file(&file).ok();
}
