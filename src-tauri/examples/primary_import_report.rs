//! Developer inspection of an explicit database path. Opens read-only and emits
//! ownership/recovery counts, never lesson bodies, answers or runner credentials.
use principia_desk_lib::storage::primary_import;
use rusqlite::{Connection, OpenFlags};
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("Usage: primary_import_report DATABASE")?);
    if args.next().is_some() {
        return Err("Usage: primary_import_report DATABASE".into());
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let plan = primary_import::inspect(&conn)?;
    let rows: BTreeMap<_, _> = plan
        .tables
        .iter()
        .map(|(name, rows)| (name, rows.len()))
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "format_version": plan.format_version,
            "source_schema_version": plan.source_schema_version,
            "fingerprint": plan.fingerprint,
            "sessions": plan.sessions,
            "unattached_course_ids": plan.unattached_course_ids,
            "source_row_counts": rows,
        }))?
    );
    Ok(())
}
