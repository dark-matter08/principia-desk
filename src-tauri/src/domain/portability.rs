//! Profile export and import.
//!
//! The desk is local-only: a lost disk is a lost learning record, and a new
//! machine has no way to inherit one. An archive is a single JSON document
//! holding every learner table with its own column list, so it survives an
//! added column without silently dropping data, and a checksum over the
//! canonical payload so a truncated or edited file is refused rather than
//! half-applied.
//!
//! An import replaces the profile. Merging two histories would have to
//! reconcile autoincrement identities across machines, and joining the wrong
//! rows silently is worse than refusing: a restore is the honest operation.
//! The caller backs the database up first; this module never deletes without
//! one.

use crate::db::{DbError, Result};
use rusqlite::{types::ValueRef, Connection, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const FORMAT: &str = "principia-desk.profile";
/// Layout of the archive itself, independent of the database schema version.
pub const SCHEMA_VERSION: u32 = 1;

/// Tables deliberately left out of an archive.
///
/// The migration ledger describes the database that receives the import, not
/// the one that produced it. The three job queues are in-flight work for a
/// provider on one machine; carrying them would resume a job whose runtime,
/// keys and files are not there.
const SKIPPED: &[&str] = &[
    "schema_migrations",
    "generation_jobs",
    "study_preparation_jobs",
    "agent_calls",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Archive {
    pub format: String,
    pub schema_version: u32,
    pub app_version: String,
    /// `PRAGMA user_version` of the exporting database. An import requires the
    /// same number: rows written by a newer schema cannot be understood here,
    /// and rows written by an older one have not been migrated.
    pub database_version: u32,
    pub exported_at: String,
    pub checksum: String,
    pub tables: BTreeMap<String, Table>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchiveSummary {
    pub exported_at: String,
    pub app_version: String,
    pub database_version: u32,
    pub tables: usize,
    pub rows: usize,
    /// Rows in the tables a learner recognises, for an honest confirmation.
    pub classes: usize,
    pub lessons: usize,
    pub schedules: usize,
}

/// Hex-encoded bytes, so a blob survives a round trip through JSON.
fn blob_value(bytes: &[u8]) -> Value {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    json!({ "$blob": hex })
}

fn decode_blob(value: &Map<String, Value>) -> Result<Vec<u8>> {
    let hex = value
        .get("$blob")
        .and_then(Value::as_str)
        .ok_or_else(|| DbError::Invalid("archive holds an unreadable value".into()))?;
    if hex.len() % 2 != 0 {
        return Err(DbError::Invalid("archive holds a malformed value".into()));
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|_| DbError::Invalid("archive holds a malformed value".into()))
        })
        .collect()
}

fn cell(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(v) => json!(v),
        ValueRef::Real(v) => json!(v),
        ValueRef::Text(v) => json!(String::from_utf8_lossy(v)),
        ValueRef::Blob(v) => blob_value(v),
    }
}

fn bind(value: &Value) -> Result<rusqlite::types::Value> {
    Ok(match value {
        Value::Null => rusqlite::types::Value::Null,
        Value::Bool(v) => rusqlite::types::Value::Integer(i64::from(*v)),
        Value::Number(v) if v.is_i64() => rusqlite::types::Value::Integer(v.as_i64().unwrap()),
        Value::Number(v) => rusqlite::types::Value::Real(
            v.as_f64()
                .ok_or_else(|| DbError::Invalid("archive holds an unreadable number".into()))?,
        ),
        Value::String(v) => rusqlite::types::Value::Text(v.clone()),
        Value::Object(map) => rusqlite::types::Value::Blob(decode_blob(map)?),
        Value::Array(_) => {
            return Err(DbError::Invalid("archive holds an unreadable value".into()))
        }
    })
}

/// Every learner table in this database, in name order.
fn exportable_tables(conn: &Connection) -> Result<Vec<String>> {
    let mut statement = conn.prepare(
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(names
        .into_iter()
        .filter(|name| !SKIPPED.contains(&name.as_str()))
        .collect())
}

fn columns_of(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(columns)
}

fn checksum(tables: &BTreeMap<String, Table>) -> Result<String> {
    // serde_json sorts nothing for us, but a BTreeMap serialises in key order
    // and every row keeps its authored column order, so the bytes are stable.
    let canonical = serde_json::to_vec(tables)?;
    let mut hash = Sha256::new();
    hash.update(&canonical);
    Ok(format!("{:x}", hash.finalize()))
}

fn user_version(conn: &Connection) -> Result<u32> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))? as u32)
}

/// Read the whole profile into an archive. Read-only.
pub fn export(conn: &Connection, app_version: &str) -> Result<Archive> {
    let mut tables = BTreeMap::new();
    for name in exportable_tables(conn)? {
        let columns = columns_of(conn, &name)?;
        if columns.is_empty() {
            continue;
        }
        let list = columns
            .iter()
            .map(|column| format!("\"{column}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let mut statement = conn.prepare(&format!("SELECT {list} FROM \"{name}\""))?;
        let rows = statement
            .query_map([], |row| {
                Ok((0..columns.len())
                    .map(|index| cell(row.get_ref_unwrap(index)))
                    .collect::<Vec<_>>())
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        tables.insert(name, Table { columns, rows });
    }
    let checksum = checksum(&tables)?;
    Ok(Archive {
        format: FORMAT.into(),
        schema_version: SCHEMA_VERSION,
        app_version: app_version.into(),
        database_version: user_version(conn)?,
        exported_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        checksum,
        tables,
    })
}

fn rows_in(archive: &Archive, table: &str) -> usize {
    archive.tables.get(table).map_or(0, |t| t.rows.len())
}

pub fn summarize(archive: &Archive) -> ArchiveSummary {
    ArchiveSummary {
        exported_at: archive.exported_at.clone(),
        app_version: archive.app_version.clone(),
        database_version: archive.database_version,
        tables: archive.tables.len(),
        rows: archive.tables.values().map(|t| t.rows.len()).sum(),
        classes: rows_in(archive, "classroom_programs"),
        lessons: rows_in(archive, "study_sessions")
            + rows_in(archive, "classroom_sessions")
            + rows_in(archive, "language_sessions"),
        schedules: rows_in(archive, "classroom_schedule_slots"),
    }
}

/// Parse and verify an archive without touching the database.
pub fn read(document: &str) -> Result<Archive> {
    let archive: Archive = serde_json::from_str(document)
        .map_err(|_| DbError::Invalid("this file is not a Principia Desk archive".into()))?;
    if archive.format != FORMAT {
        return Err(DbError::Invalid(
            "this file is not a Principia Desk archive".into(),
        ));
    }
    if archive.schema_version > SCHEMA_VERSION {
        return Err(DbError::Invalid(format!(
            "this archive was written by a newer version of the app (archive format {}, this build reads {SCHEMA_VERSION})",
            archive.schema_version
        )));
    }
    if checksum(&archive.tables)? != archive.checksum {
        return Err(DbError::Invalid(
            "this archive is damaged or was edited after it was written; its contents no longer match its checksum".into(),
        ));
    }
    Ok(archive)
}

fn restore_into(tx: &Transaction<'_>, archive: &Archive) -> Result<usize> {
    let present = exportable_tables(tx)?;
    // Deleting in reverse and inserting with deferred foreign keys frees the
    // import from knowing the dependency order, and the check below still
    // refuses an archive whose rows do not hang together.
    for name in present.iter().rev() {
        tx.execute(&format!("DELETE FROM \"{name}\""), [])?;
    }
    let mut written = 0usize;
    for (name, table) in &archive.tables {
        if !present.contains(name) {
            return Err(DbError::Invalid(format!(
                "this archive holds a table this build does not have ({name}); update the app and import again"
            )));
        }
        let current = columns_of(tx, name)?;
        let usable: Vec<usize> = table
            .columns
            .iter()
            .enumerate()
            .filter(|(_, column)| current.contains(column))
            .map(|(index, _)| index)
            .collect();
        if usable.is_empty() || table.rows.is_empty() {
            continue;
        }
        let names = usable
            .iter()
            .map(|index| format!("\"{}\"", table.columns[*index]))
            .collect::<Vec<_>>()
            .join(", ");
        let marks = usable
            .iter()
            .enumerate()
            .map(|(position, _)| format!("?{}", position + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let mut statement = tx.prepare(&format!(
            "INSERT INTO \"{name}\" ({names}) VALUES ({marks})"
        ))?;
        for row in &table.rows {
            if row.len() != table.columns.len() {
                return Err(DbError::Invalid(format!(
                    "this archive holds a malformed row in {name}"
                )));
            }
            let values = usable
                .iter()
                .map(|index| bind(&row[*index]))
                .collect::<Result<Vec<_>>>()?;
            statement.execute(rusqlite::params_from_iter(values))?;
            written += 1;
        }
    }
    Ok(written)
}

/// Replace this profile with the archive's contents, in one transaction.
///
/// The caller must have published a backup first: on any failure the
/// transaction rolls back, but a learner who imports the wrong file is owed a
/// way back to the record they had a moment ago.
pub fn restore(conn: &mut Connection, archive: &Archive) -> Result<usize> {
    let current = user_version(conn)?;
    if archive.database_version != current {
        return Err(DbError::Invalid(format!(
            "this archive came from database version {} and this build is on {current}; open it with a matching build",
            archive.database_version
        )));
    }
    conn.pragma_update(None, "defer_foreign_keys", true)?;
    let result = (|| -> Result<usize> {
        let tx = conn.transaction()?;
        let written = restore_into(&tx, archive)?;
        let violations: i64 =
            tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        if violations > 0 {
            return Err(DbError::Invalid(
                "this archive's records do not hang together; nothing was changed".into(),
            ));
        }
        let integrity: String = tx.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(DbError::Invalid(format!(
                "the database failed its integrity check after the import ({integrity}); nothing was changed"
            )));
        }
        tx.commit()?;
        Ok(written)
    })();
    let _ = conn.pragma_update(None, "defer_foreign_keys", false);
    result
}
