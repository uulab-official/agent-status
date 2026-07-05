use rusqlite::Connection;

const SCHEMA_SQL: &str = include_str!("schema.sql");
const SCHEMA_VERSION: i64 = 1;

/// Opens (or creates) the Agent Status SQLite database and applies the schema.
/// Pass `:memory:` in tests; pass a real path (the app's data directory) in
/// the desktop app.
pub fn open_database(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.execute_batch(SCHEMA_SQL)?;

    let current_version: Option<i64> =
        conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row.get(0))?;

    if current_version.unwrap_or(0) < SCHEMA_VERSION {
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![SCHEMA_VERSION, chrono::Utc::now().to_rfc3339()],
        )?;
    }

    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_the_schema_and_records_the_migration_version() {
        let conn = open_database(":memory:").unwrap();
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").unwrap();
        let names: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().filter_map(Result::ok).collect();
        for expected in ["providers", "usage_history", "cost_history", "notifications", "events", "settings"] {
            assert!(names.contains(&expected.to_string()), "missing table {expected}");
        }

        let version: i64 = conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0)).unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn is_idempotent_across_repeated_opens() {
        let conn1 = open_database(":memory:").unwrap();
        drop(conn1);
        let conn2 = open_database(":memory:").unwrap();
        conn2.execute("SELECT 1 FROM providers", []).ok();
    }
}
