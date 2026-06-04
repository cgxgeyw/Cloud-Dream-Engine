pub mod migrations;
pub mod repositories;
pub mod schema;
pub mod seeds;

use rusqlite::Connection;
use std::path::PathBuf;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new(data_dir: &PathBuf) -> Result<Self, String> {
        let db_path = data_dir.join("dream_narrative_engine.db");
        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| e.to_string())?;

        let db = Database { conn };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&self) -> Result<(), String> {
        schema::create_tables(&self.conn).map_err(|e| e.to_string())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
