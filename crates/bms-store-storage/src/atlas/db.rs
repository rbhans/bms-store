use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};

use super::model::{AtlasEquipment, AtlasPoint, AtlasStats};

/// Read-only handle to a BAS Atlas SQLite database.
pub struct AtlasDb {
    conn: Connection,
}

/// An alias row: maps a normalized alias string to a point or equipment ID.
#[derive(Debug, Clone)]
pub struct AliasRow {
    pub alias: String,
    pub target_id: String,
}

impl AtlasDb {
    /// Open a read-only connection to the Atlas database.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(AtlasDb { conn })
    }

    /// Check if the Atlas database file exists and has a valid schema.
    pub fn is_available(path: &Path) -> bool {
        if !path.exists() {
            return false;
        }
        match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(conn) => {
                // Check for the atlas_meta table as a schema marker
                conn.query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='atlas_meta'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map(|count| count > 0)
                .unwrap_or(false)
            }
            Err(_) => false,
        }
    }

    /// Get Atlas database statistics.
    pub fn stats(&self) -> Result<AtlasStats, rusqlite::Error> {
        let version: String = self
            .conn
            .query_row(
                "SELECT value FROM atlas_meta WHERE key='version'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "unknown".into());

        let updated_ms: i64 = self
            .conn
            .query_row(
                "SELECT value FROM atlas_meta WHERE key='updated_ms'",
                [],
                |row| {
                    let s: String = row.get(0)?;
                    Ok(s.parse::<i64>().unwrap_or(0))
                },
            )
            .unwrap_or(0);

        let total_points: u32 = self
            .conn
            .query_row("SELECT count(*) FROM atlas_points", [], |row| row.get(0))
            .unwrap_or(0);

        let total_equipment: u32 = self
            .conn
            .query_row("SELECT count(*) FROM atlas_equipment", [], |row| row.get(0))
            .unwrap_or(0);

        Ok(AtlasStats {
            version,
            total_points,
            total_equipment,
            updated_ms,
        })
    }

    /// Load all point definitions.
    pub fn all_points(&self) -> Result<Vec<AtlasPoint>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, category, haystack_tags, kind, point_function, units, brick
             FROM atlas_points",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AtlasPoint {
                id: row.get(0)?,
                name: row.get(1)?,
                category: row.get(2)?,
                haystack_tags: row.get(3)?,
                kind: row.get(4)?,
                point_function: row.get(5)?,
                units: row.get(6)?,
                brick: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    /// Load all equipment definitions.
    pub fn all_equipment(&self) -> Result<Vec<AtlasEquipment>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, abbreviation, category, haystack_tags, brick
             FROM atlas_equipment",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AtlasEquipment {
                id: row.get(0)?,
                name: row.get(1)?,
                abbreviation: row.get(2)?,
                category: row.get(3)?,
                haystack_tags: row.get(4)?,
                brick: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Load all point aliases: (normalized_alias, point_id).
    pub fn all_point_aliases(&self) -> Result<Vec<AliasRow>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT alias, point_id FROM atlas_point_aliases")?;
        let rows = stmt.query_map([], |row| {
            Ok(AliasRow {
                alias: row.get(0)?,
                target_id: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// Load all equipment aliases: (normalized_alias, equip_id).
    pub fn all_equip_aliases(&self) -> Result<Vec<AliasRow>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT alias, equip_id FROM atlas_equip_aliases")?;
        let rows = stmt.query_map([], |row| {
            Ok(AliasRow {
                alias: row.get(0)?,
                target_id: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// Get typical points for an equipment type.
    pub fn equip_typical_points(&self, equip_id: &str) -> Result<Vec<String>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT point_id FROM atlas_equip_typical_points WHERE equip_id = ?1")?;
        let rows = stmt.query_map(params![equip_id], |row| row.get::<_, String>(0))?;
        rows.collect()
    }
}

/// Create the Atlas database schema in a writable connection.
/// Used by the sync module when downloading fresh data.
pub fn create_atlas_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS atlas_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS atlas_points (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            category TEXT NOT NULL DEFAULT '',
            haystack_tags TEXT NOT NULL DEFAULT '',
            kind TEXT NOT NULL DEFAULT 'Number',
            point_function TEXT NOT NULL DEFAULT 'sensor',
            units TEXT,
            brick TEXT
        );
        CREATE TABLE IF NOT EXISTS atlas_equipment (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            abbreviation TEXT,
            category TEXT NOT NULL DEFAULT '',
            haystack_tags TEXT NOT NULL DEFAULT '',
            brick TEXT
        );
        CREATE TABLE IF NOT EXISTS atlas_point_aliases (
            alias TEXT NOT NULL,
            point_id TEXT NOT NULL REFERENCES atlas_points(id),
            PRIMARY KEY (alias, point_id)
        );
        CREATE TABLE IF NOT EXISTS atlas_equip_aliases (
            alias TEXT NOT NULL,
            equip_id TEXT NOT NULL REFERENCES atlas_equipment(id),
            PRIMARY KEY (alias, equip_id)
        );
        CREATE TABLE IF NOT EXISTS atlas_equip_typical_points (
            equip_id TEXT NOT NULL REFERENCES atlas_equipment(id),
            point_id TEXT NOT NULL REFERENCES atlas_points(id),
            PRIMARY KEY (equip_id, point_id)
        );
        CREATE INDEX IF NOT EXISTS idx_point_alias ON atlas_point_aliases(alias);
        CREATE INDEX IF NOT EXISTS idx_equip_alias ON atlas_equip_aliases(alias);",
    )?;
    Ok(())
}
