-- Agent Status local database schema.
-- Kept intentionally provider-agnostic: every row is scoped by provider_id
-- (a free-text key, not a foreign key) so adding a provider never touches
-- this schema.

CREATE TABLE IF NOT EXISTS providers (
  id TEXT PRIMARY KEY,
  display_name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  config_json TEXT NOT NULL DEFAULT '{}'
);

-- One row per LimitWindow reading, sampled on each successful refresh.
-- This is what powers the Timeline / history charts.
CREATE TABLE IF NOT EXISTS usage_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  provider_id TEXT NOT NULL,
  window_id TEXT NOT NULL,
  period TEXT NOT NULL,
  unit TEXT NOT NULL,
  used REAL NOT NULL,
  limit_value REAL,
  confidence INTEGER NOT NULL,
  observed_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_usage_history_provider_time
  ON usage_history (provider_id, observed_at);

CREATE TABLE IF NOT EXISTS cost_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  provider_id TEXT NOT NULL,
  currency TEXT NOT NULL,
  amount REAL NOT NULL,
  period TEXT NOT NULL, -- 'today' | 'week' | 'month'
  confidence INTEGER NOT NULL,
  observed_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cost_history_provider_time
  ON cost_history (provider_id, observed_at);

CREATE TABLE IF NOT EXISTS notifications (
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL,
  severity TEXT NOT NULL,
  reason TEXT NOT NULL,
  message TEXT NOT NULL,
  created_at TEXT NOT NULL,
  dismissed_at TEXT
);

CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  provider_id TEXT NOT NULL,
  kind TEXT NOT NULL, -- 'state_change' | 'error' | 'plugin_loaded' | ...
  detail TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL
);
