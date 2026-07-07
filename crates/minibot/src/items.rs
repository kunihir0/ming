use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Deserialize, Clone)]
pub struct RawItemDefinition {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "DisplayName")]
    pub display_name: String,
    #[serde(rename = "ShortName")]
    pub short_name: String,
}

#[derive(Debug, Clone)]
pub struct ItemDefinition {
    pub name: String,
    pub shortname: String,
}

pub static ITEMS: LazyLock<HashMap<String, ItemDefinition>> = LazyLock::new(|| {
    let json = include_str!("../items.json");
    let raw_items: Vec<RawItemDefinition> =
        serde_json::from_str(json).expect("Failed to parse items.json");

    let mut map = HashMap::new();
    for raw in raw_items {
        map.insert(
            raw.id.to_string(),
            ItemDefinition {
                name: raw.display_name,
                shortname: raw.short_name,
            },
        );
    }
    map
});

/// Smart item search supporting regex and partial matches.
/// Returns a list of (id, item_name).
pub fn search_items_smart(query: &str) -> Vec<(i32, String, String)> {
    let mut results = Vec::new();

    // Check if query is a regex, e.g., ^rifle.*
    let is_regex =
        query.contains('*') || query.contains('^') || query.contains('$') || query.contains('[');

    // Create a compiled regex if it's a regex-like query
    let re = if is_regex {
        // Convert typical wildcard * to .* for ease of use
        let regex_str = if query.contains('*') && !query.contains(".*") {
            query.replace('*', ".*")
        } else {
            query.to_string()
        };
        Regex::new(&format!("(?i){regex_str}")).ok()
    } else {
        None
    };

    let lower_query = query.to_lowercase();

    for (id_str, item) in ITEMS.iter() {
        let id_num = id_str.parse::<i32>().unwrap_or(0);
        let mut matches = false;

        if let Some(regex) = &re {
            if regex.is_match(&item.name) || regex.is_match(&item.shortname) {
                matches = true;
            }
        } else {
            let lower_name = item.name.to_lowercase();
            let lower_shortname = item.shortname.to_lowercase();

            if lower_name.contains(&lower_query) || lower_shortname.contains(&lower_query) {
                matches = true;
            }
        }

        if matches {
            results.push((id_num, item.name.clone(), item.shortname.clone()));
        }
    }

    results
}

pub fn get_item_name(id: i32) -> String {
    let id_str = id.to_string();
    ITEMS.get(&id_str).map_or(id_str, |item| item.name.clone())
}

pub fn get_item_shortname(id: i32) -> Option<String> {
    let id_str = id.to_string();
    ITEMS.get(&id_str).map(|item| item.shortname.clone())
}
