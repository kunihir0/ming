pub mod models;
pub mod schema;

use diesel::sqlite::SqliteConnection;
use diesel::r2d2::{ConnectionManager, Pool};

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

/// Establishes the database connection pool.
///
/// # Panics
/// Panics if the connection pool cannot be created.
#[must_use]
pub fn establish_connection_pool(database_url: &str) -> DbPool {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    Pool::builder()
        .build(manager)
        .expect("Failed to create database pool.")
}
