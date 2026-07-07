use anyhow::Result;
use chrono::{Datelike, Duration, Timelike, Utc};
use db::models::{PlayerSession, TrackedPlayer};
use db::schema::player_sessions::dsl::*;
use db::DbPool;
use diesel::prelude::*;
use std::collections::HashMap;

pub struct AnalyticsData {
    pub total_hours: f64,
    pub session_count: i32,
    pub avg_session_mins: f64,
    pub peak_time_hour: Option<u32>,        // 0-23
    pub daily_playtime: Vec<(String, f64)>, // Date string to hours
}

pub fn get_player_analytics(pool: &DbPool, p_id: i32) -> Result<AnalyticsData> {
    let mut conn = pool.get()?;

    let thirty_days_ago = Utc::now().naive_utc() - Duration::days(30);

    let sessions: Vec<PlayerSession> = player_sessions
        .filter(tracked_player_id.eq(p_id))
        .filter(joined_at.ge(thirty_days_ago))
        .load(&mut conn)?;

    let mut total_secs = 0;
    let mut daily_map: HashMap<String, i32> = HashMap::new();
    let mut hour_counts: HashMap<u32, i32> = HashMap::new();

    for s in &sessions {
        // If they are still online, calculate duration up to now
        let end_time = s.left_at.unwrap_or_else(|| Utc::now().naive_utc());
        let dur = end_time.signed_duration_since(s.joined_at).num_seconds() as i32;

        total_secs += dur;
        let date_str = s.joined_at.date().format("%Y-%m-%d").to_string();
        *daily_map.entry(date_str).or_insert(0) += dur;

        let hour = s.joined_at.hour();
        *hour_counts.entry(hour).or_insert(0) += 1;
    }

    let mut peak_time_hour = None;
    let mut max_count = 0;
    for (h, c) in hour_counts {
        if c > max_count {
            max_count = c;
            peak_time_hour = Some(h);
        }
    }

    let mut daily_vec: Vec<(String, f64)> = daily_map
        .into_iter()
        .map(|(k, v)| (k, (v as f64) / 3600.0))
        .collect();

    daily_vec.sort_by(|a, b| a.0.cmp(&b.0));

    let session_count = sessions.len() as i32;
    let avg_session_mins = if session_count > 0 {
        (total_secs as f64) / (session_count as f64) / 60.0
    } else {
        0.0
    };

    Ok(AnalyticsData {
        total_hours: (total_secs as f64) / 3600.0,
        session_count,
        avg_session_mins,
        peak_time_hour,
        daily_playtime: daily_vec,
    })
}
