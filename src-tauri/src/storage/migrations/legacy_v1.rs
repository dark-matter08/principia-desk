//! Frozen migration for the schemas shipped before numbered migrations.
//! Changes belong in a new migration: this source is part of v1's checksum.
use crate::db::Result;
use rusqlite::Connection;
use serde::Deserialize;

pub const BASELINE: &str = include_str!("001_legacy_baseline.sql");
pub const COLUMNS: &str = include_str!("001_legacy_columns.json");
pub const COURSES: &str = include_str!("001_course_sources.sql");
pub const EXERCISES: &str = include_str!("001_exercise_owners.sql");

pub fn apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(BASELINE)?;
    #[derive(Deserialize)]
    struct Column {
        table: String,
        column: String,
        sql: String,
    }
    for column in serde_json::from_str::<Vec<Column>>(COLUMNS)? {
        let present: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
            [&column.table, &column.column],
            |row| row.get(0),
        )?;
        if !present {
            conn.execute_batch(&column.sql)?;
        }
    }
    let schema: String = conn.query_row(
        "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'courses'",
        [],
        |row| row.get(0),
    )?;
    if !schema.contains("'deepseek'") || !schema.contains("'custom'") {
        conn.execute_batch(COURSES)?;
    }
    let unified: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('exercise_drafts') WHERE name = 'classroom_session_id')", [], |row| row.get(0))?;
    if !unified {
        conn.execute_batch(EXERCISES)?;
    }
    Ok(())
}
