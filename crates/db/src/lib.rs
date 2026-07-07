pub mod models;
pub mod schema;

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Run migrations on the database.
///
/// # Errors
/// Returns an error if migrations fail to run.
pub fn run_migrations(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| anyhow::anyhow!("Migration error: {e}"))?;
    Ok(())
}

#[derive(Debug)]
pub struct ConnectionCustomizer;

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error>
    for ConnectionCustomizer
{
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        // Set a busy timeout to handle lock contention
        conn.batch_execute("PRAGMA busy_timeout = 5000;")
            .map_err(diesel::r2d2::Error::QueryError)?;
        Ok(())
    }
}

/// Establishes the database connection pool.
///
/// # Panics
/// Panics if the connection pool cannot be created.
#[must_use]
pub fn establish_connection_pool(database_url: &str) -> DbPool {
    // 1. Open a single connection to set WAL mode before the pool starts
    if let Ok(mut conn) = SqliteConnection::establish(database_url) {
        let _ = conn.batch_execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;");
    }

    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    Pool::builder()
        .connection_customizer(Box::new(ConnectionCustomizer))
        .max_size(10)
        .build(manager)
        .expect("Failed to create database pool.")
}

/// Upsert a player link mapping between `steam_id` and `bm_id`.
///
/// # Errors
/// Returns an error if the database query fails.
pub fn upsert_player_link(
    conn: &mut SqliteConnection,
    steam_id: &str,
    bm_id: &str,
) -> QueryResult<usize> {
    use crate::schema::player_links::dsl as pl;
    use chrono::Utc;

    let now = Utc::now().naive_utc();
    let new_link = models::NewPlayerLink { steam_id, bm_id };

    diesel::insert_into(pl::player_links)
        .values(&new_link)
        .on_conflict(pl::steam_id)
        .do_update()
        .set((pl::bm_id.eq(bm_id), pl::updated_at.eq(now)))
        .execute(conn)
}

/// Get the `BattleMetrics` ID for a given Steam ID.
///
/// # Errors
/// Returns an error if the database query fails.
pub fn get_bm_id_for_steam_id(
    conn: &mut SqliteConnection,
    steam_id_val: &str,
) -> QueryResult<Option<String>> {
    use crate::schema::player_links::dsl as pl;

    pl::player_links
        .filter(pl::steam_id.eq(steam_id_val))
        .select(pl::bm_id)
        .first::<String>(conn)
        .optional()
}
