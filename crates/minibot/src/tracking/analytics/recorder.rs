use anyhow::Result;
use diesel::prelude::*;
use db::DbPool;
use db::models::{NewPlayerSession, PlayerSession};
use db::schema::player_sessions::dsl::*;
use chrono::Utc;

pub fn start_session(pool: &DbPool, player_id: i32, srv_id: i32, steam_id_val: &str) -> Result<()> {
    let mut conn = pool.get()?;
    
    // Check if there's already an active session
    let existing: Option<PlayerSession> = player_sessions
        .filter(tracked_player_id.eq(player_id))
        .filter(left_at.is_null())
        .first(&mut conn)
        .optional()?;
        
    if existing.is_none() {
        let new_sess = NewPlayerSession {
            tracked_player_id: player_id,
            server_id: srv_id,
            steam_id: steam_id_val.to_string(),
        };
        diesel::insert_into(player_sessions)
            .values(&new_sess)
            .execute(&mut conn)?;
    }
    
    Ok(())
}

pub fn end_session(pool: &DbPool, player_id: i32) -> Result<()> {
    let mut conn = pool.get()?;
    
    let now = Utc::now().naive_utc();
    
    // Find active sessions for this player and close them
    let active_sessions: Vec<PlayerSession> = player_sessions
        .filter(tracked_player_id.eq(player_id))
        .filter(left_at.is_null())
        .load(&mut conn)?;
        
    for sess in active_sessions {
        let duration = now.signed_duration_since(sess.joined_at).num_seconds() as i32;
        diesel::update(player_sessions.filter(id.eq(sess.id)))
            .set((
                left_at.eq(Some(now)),
                duration_secs.eq(Some(duration))
            ))
            .execute(&mut conn)?;
    }
    
    Ok(())
}
