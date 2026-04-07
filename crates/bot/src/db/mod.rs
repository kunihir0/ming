pub mod models;
pub mod schema;

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

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
