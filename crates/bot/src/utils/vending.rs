use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub struct ItemDefinition {
    pub name: String,
    pub shortname: String,
}

pub static ITEMS: LazyLock<HashMap<String, ItemDefinition>> = LazyLock::new(|| {
    let json = include_str!("items.json");
    serde_json::from_str(json).expect("Failed to parse items.json")
});

#[must_use]
pub fn resolve_item_id(query: &str) -> Option<String> {
    let lower_query = query.to_lowercase();

    // 1. Direct ID check
    if ITEMS.contains_key(&lower_query) {
        return Some(lower_query);
    }

    // 2. Name search (exact or partial)
    let mut best_match: Option<(String, usize)> = None;

    for (id, item) in ITEMS.iter() {
        let lower_name = item.name.to_lowercase();
        if lower_name == lower_query {
            return Some(id.clone());
        }
        if lower_name.contains(&lower_query) {
            match best_match {
                Some((_, len)) if lower_name.len() < len => {
                    best_match = Some((id.clone(), lower_name.len()));
                }
                None => {
                    best_match = Some((id.clone(), lower_name.len()));
                }
                _ => {}
            }
        }
    }

    best_match.map(|(id, _)| id)
}

#[must_use]
pub fn get_item_name(id: i32) -> String {
    let id_str = id.to_string();
    ITEMS.get(&id_str).map_or(id_str, |item| item.name.clone())
}
