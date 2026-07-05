use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};

/// Typed key-value settings backed by the `settings` table. Values are stored
/// as JSON so callers don't need a column per setting — the schema doesn't
/// change when a new preference is added.
pub fn get_setting<T: DeserializeOwned>(conn: &Connection, key: &str, fallback: T) -> T {
    let raw: Option<String> = conn
        .query_row("SELECT value_json FROM settings WHERE key = ?1", params![key], |row| row.get(0))
        .optional()
        .unwrap_or(None);

    match raw {
        Some(json) => serde_json::from_str(&json).unwrap_or(fallback),
        None => fallback,
    }
}

pub fn set_setting<T: Serialize>(conn: &Connection, key: &str, value: &T) -> rusqlite::Result<()> {
    let json = serde_json::to_string(value).expect("setting value must serialize to JSON");
    conn.execute(
        "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        params![key, json],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_database;

    #[test]
    fn returns_the_fallback_when_a_key_is_unset() {
        let conn = open_database(":memory:").unwrap();
        assert_eq!(get_setting(&conn, "trayMode", "compact".to_string()), "compact");
    }

    #[test]
    fn round_trips_a_value_through_json() {
        let conn = open_database(":memory:").unwrap();
        set_setting(&conn, "launchAtLogin", &true).unwrap();
        assert!(get_setting(&conn, "launchAtLogin", false));
    }

    #[test]
    fn overwrites_an_existing_value() {
        let conn = open_database(":memory:").unwrap();
        set_setting(&conn, "trayMode", &"compact".to_string()).unwrap();
        set_setting(&conn, "trayMode", &"detailed".to_string()).unwrap();
        assert_eq!(get_setting(&conn, "trayMode", "compact".to_string()), "detailed");
    }
}
