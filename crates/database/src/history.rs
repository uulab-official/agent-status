use rusqlite::{params, Connection};

/// One `LimitWindow` reading, sampled on a successful refresh. Deliberately
/// takes primitive fields rather than a `LimitWindow` from `agent-core` —
/// this crate has no dependency on the standard model, so it stays a pure
/// storage layer that any caller can feed without a new dependency edge.
pub struct UsageRecord<'a> {
    pub provider_id: &'a str,
    pub window_id: &'a str,
    pub period: &'a str,
    pub unit: &'a str,
    pub used: f64,
    pub limit_value: Option<f64>,
    pub confidence: u8,
    pub observed_at: &'a str,
}

pub fn record_usage(conn: &Connection, record: &UsageRecord) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO usage_history (provider_id, window_id, period, unit, used, limit_value, confidence, observed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            record.provider_id,
            record.window_id,
            record.period,
            record.unit,
            record.used,
            record.limit_value,
            record.confidence,
            record.observed_at,
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageHistoryRow {
    pub window_id: String,
    pub period: String,
    pub unit: String,
    pub used: f64,
    pub limit_value: Option<f64>,
    pub confidence: u8,
    pub observed_at: String,
}

/// Most recent readings for one provider, newest first.
pub fn recent_usage(conn: &Connection, provider_id: &str, limit: u32) -> rusqlite::Result<Vec<UsageHistoryRow>> {
    let mut stmt = conn.prepare(
        "SELECT window_id, period, unit, used, limit_value, confidence, observed_at
         FROM usage_history WHERE provider_id = ?1 ORDER BY observed_at DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![provider_id, limit], |row| {
            Ok(UsageHistoryRow {
                window_id: row.get(0)?,
                period: row.get(1)?,
                unit: row.get(2)?,
                used: row.get(3)?,
                limit_value: row.get(4)?,
                confidence: row.get(5)?,
                observed_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// One `CostSnapshot` component (a provider reports up to three at once:
/// today/this_week/this_month), sampled on a successful refresh.
pub struct CostRecord<'a> {
    pub provider_id: &'a str,
    pub currency: &'a str,
    pub amount: f64,
    /// "today" | "week" | "month"
    pub period: &'a str,
    pub confidence: u8,
    pub observed_at: &'a str,
}

pub fn record_cost(conn: &Connection, record: &CostRecord) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO cost_history (provider_id, currency, amount, period, confidence, observed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![record.provider_id, record.currency, record.amount, record.period, record.confidence, record.observed_at],
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostHistoryRow {
    pub currency: String,
    pub amount: f64,
    pub period: String,
    pub confidence: u8,
    pub observed_at: String,
}

pub fn recent_cost(conn: &Connection, provider_id: &str, limit: u32) -> rusqlite::Result<Vec<CostHistoryRow>> {
    let mut stmt = conn.prepare(
        "SELECT currency, amount, period, confidence, observed_at
         FROM cost_history WHERE provider_id = ?1 ORDER BY observed_at DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![provider_id, limit], |row| {
            Ok(CostHistoryRow {
                currency: row.get(0)?,
                amount: row.get(1)?,
                period: row.get(2)?,
                confidence: row.get(3)?,
                observed_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_database;

    #[test]
    fn records_and_reads_back_usage_newest_first() {
        let conn = open_database(":memory:").unwrap();
        record_usage(
            &conn,
            &UsageRecord {
                provider_id: "claude",
                window_id: "session",
                period: "session",
                unit: "messages",
                used: 40.0,
                limit_value: Some(100.0),
                confidence: 3,
                observed_at: "2026-01-01T00:00:00Z",
            },
        )
        .unwrap();
        record_usage(
            &conn,
            &UsageRecord {
                provider_id: "claude",
                window_id: "session",
                period: "session",
                unit: "messages",
                used: 55.0,
                limit_value: Some(100.0),
                confidence: 3,
                observed_at: "2026-01-01T01:00:00Z",
            },
        )
        .unwrap();

        let rows = recent_usage(&conn, "claude", 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].used, 55.0, "newest reading should come first");
        assert_eq!(rows[1].used, 40.0);
    }

    #[test]
    fn usage_history_is_scoped_per_provider() {
        let conn = open_database(":memory:").unwrap();
        record_usage(
            &conn,
            &UsageRecord {
                provider_id: "claude",
                window_id: "session",
                period: "session",
                unit: "messages",
                used: 10.0,
                limit_value: None,
                confidence: 3,
                observed_at: "2026-01-01T00:00:00Z",
            },
        )
        .unwrap();
        record_usage(
            &conn,
            &UsageRecord {
                provider_id: "openrouter",
                window_id: "credit",
                period: "fixed",
                unit: "usd",
                used: 4.2,
                limit_value: Some(20.0),
                confidence: 5,
                observed_at: "2026-01-01T00:00:00Z",
            },
        )
        .unwrap();

        assert_eq!(recent_usage(&conn, "claude", 10).unwrap().len(), 1);
        assert_eq!(recent_usage(&conn, "openrouter", 10).unwrap().len(), 1);
        assert_eq!(recent_usage(&conn, "nonexistent", 10).unwrap().len(), 0);
    }

    #[test]
    fn respects_the_row_limit() {
        let conn = open_database(":memory:").unwrap();
        for i in 0..5 {
            let observed_at = format!("2026-01-01T0{i}:00:00Z");
            record_usage(
                &conn,
                &UsageRecord {
                    provider_id: "claude",
                    window_id: "session",
                    period: "session",
                    unit: "messages",
                    used: i as f64,
                    limit_value: None,
                    confidence: 3,
                    observed_at: &observed_at,
                },
            )
            .unwrap();
        }
        assert_eq!(recent_usage(&conn, "claude", 3).unwrap().len(), 3);
    }

    #[test]
    fn records_and_reads_back_cost() {
        let conn = open_database(":memory:").unwrap();
        record_cost(
            &conn,
            &CostRecord { provider_id: "openrouter", currency: "usd", amount: 12.42, period: "month", confidence: 5, observed_at: "2026-01-01T00:00:00Z" },
        )
        .unwrap();

        let rows = recent_cost(&conn, "openrouter", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].amount, 12.42);
        assert_eq!(rows[0].period, "month");
    }
}
