//! Lightweight SQLite schema migration runner.
//!
//! Each store defines a `&[Migration]` list. On startup, the runner creates a
//! `schema_version` table (if absent), checks the current version, and applies
//! any unapplied migrations in order.
//!
//! Migrations are **forward-only** — there is no downgrade path. Each migration
//! receives the database connection and executes arbitrary SQL.

use rusqlite::Connection;

/// A single forward migration.
pub struct Migration {
    /// Sequential version number (1-based). Must be unique and monotonically increasing.
    pub version: u32,
    /// Human-readable label for logging / the schema_version table.
    pub label: &'static str,
    /// SQL to execute for this migration. May contain multiple statements.
    pub sql: &'static str,
}

/// Ensure the `schema_version` table exists, then apply any unapplied migrations
/// from `migrations` in order. Returns the final schema version.
///
/// Returns an error if a migration fails rather than panicking, so callers
/// can surface a meaningful message to the user.
pub fn run_migrations(
    conn: &Connection,
    store_name: &str,
    migrations: &[Migration],
) -> Result<u32, String> {
    // Create version tracking table (idempotent).
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version     INTEGER PRIMARY KEY,
            label       TEXT NOT NULL,
            applied_ms  INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("{store_name}: failed to create schema_version table: {e}"))?;

    // Current version.
    let current: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for m in migrations {
        if m.version <= current {
            continue;
        }
        // Apply inside a transaction so partial failures don't leave inconsistent state.
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("{store_name}: failed to begin migration transaction: {e}"))?;
        tx.execute_batch(m.sql).map_err(|e| {
            format!(
                "{store_name}: migration v{} ({}) failed: {e}",
                m.version, m.label
            )
        })?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        tx.execute(
            "INSERT INTO schema_version (version, label, applied_ms) VALUES (?1, ?2, ?3)",
            rusqlite::params![m.version, m.label, now_ms],
        )
        .map_err(|e| format!("{store_name}: failed to record migration version: {e}"))?;
        tx.commit()
            .map_err(|e| format!("{store_name}: failed to commit migration: {e}"))?;
        tracing::info!(store = store_name, version = m.version, label = %m.label, "Applied schema migration");
    }

    let final_version: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(final_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_in_order() {
        let conn = Connection::open_in_memory().unwrap();
        let migrations = &[
            Migration {
                version: 1,
                label: "create foo",
                sql: "CREATE TABLE foo (id INTEGER PRIMARY KEY, name TEXT);",
            },
            Migration {
                version: 2,
                label: "add bar column",
                sql: "ALTER TABLE foo ADD COLUMN bar TEXT;",
            },
        ];

        let v = run_migrations(&conn, "test", migrations).unwrap();
        assert_eq!(v, 2);

        // Verify table has both columns.
        conn.execute("INSERT INTO foo (id, name, bar) VALUES (1, 'a', 'b')", [])
            .unwrap();
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        let migrations = &[Migration {
            version: 1,
            label: "create foo",
            sql: "CREATE TABLE foo (id INTEGER PRIMARY KEY);",
        }];

        let v1 = run_migrations(&conn, "test", migrations).unwrap();
        assert_eq!(v1, 1);

        // Running again should be a no-op.
        let v2 = run_migrations(&conn, "test", migrations).unwrap();
        assert_eq!(v2, 1);
    }

    #[test]
    fn incremental_migration() {
        let conn = Connection::open_in_memory().unwrap();

        // First deploy: v1 only.
        let m1 = &[Migration {
            version: 1,
            label: "create items",
            sql: "CREATE TABLE items (id INTEGER PRIMARY KEY);",
        }];
        assert_eq!(run_migrations(&conn, "test", m1).unwrap(), 1);

        // Second deploy: v1 + v2.
        let m2 = &[
            Migration {
                version: 1,
                label: "create items",
                sql: "CREATE TABLE items (id INTEGER PRIMARY KEY);",
            },
            Migration {
                version: 2,
                label: "add name",
                sql: "ALTER TABLE items ADD COLUMN name TEXT;",
            },
        ];
        assert_eq!(run_migrations(&conn, "test", m2).unwrap(), 2);

        conn.execute("INSERT INTO items (id, name) VALUES (1, 'test')", [])
            .unwrap();
    }
}
