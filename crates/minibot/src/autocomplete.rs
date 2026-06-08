use crate::Error;
use db::models::PairedServer;
use diesel::prelude::*;
use poise::serenity_prelude as serenity;
use std::sync::Arc;

pub async fn autocomplete_server<'a>(
    ctx: poise::Context<'a, Arc<crate::framework::MinibotData>, Error>,
    partial: &'a str,
) -> impl std::iter::Iterator<Item = serenity::AutocompleteChoice> + 'a {
    let mut conn = match ctx.data().db_pool.get() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to get db connection for autocomplete: {}", e);
            return vec![].into_iter();
        }
    };
    use db::schema::paired_servers::dsl::*;
    
    let servers: Vec<PairedServer> = paired_servers.load(&mut conn).unwrap_or_default();
    
    servers
        .into_iter()
        .filter(move |s| {
            partial.is_empty() 
            || s.name.to_lowercase().contains(&partial.to_lowercase()) 
            || s.id.to_string().contains(partial)
        })
        .take(25)
        .map(|s| {
            serenity::AutocompleteChoice::new(
                format!("{} (ID: {})", s.name, s.id),
                s.id as i64,
            )
        })
        .collect::<Vec<_>>()
        .into_iter()
}

pub async fn autocomplete_item<'a>(
    _ctx: poise::ApplicationContext<'a, Arc<crate::framework::MinibotData>, Error>,
    partial: &'a str,
) -> impl std::iter::Iterator<Item = serenity::AutocompleteChoice> + 'a {
    if partial.is_empty() {
        return vec![].into_iter();
    }

    let results = crate::items::search_items_smart(partial);
    
    results
        .into_iter()
        .take(25)
        .map(|(_id, name, _shortname)| {
            serenity::AutocompleteChoice::new(name.clone(), name)
        })
        .collect::<Vec<_>>()
        .into_iter()
}
