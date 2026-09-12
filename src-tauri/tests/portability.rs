//! A local-only desk has to be able to leave the machine and come back.

use principia_desk_lib::{
    db,
    domain::portability::{self, Archive},
};
use rusqlite::Connection;

fn path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "principia-portability-{name}-{}-{:032x}.db",
        std::process::id(),
        rand::random::<u128>()
    ))
}

/// A profile with a class, a schedule and a finished lesson behind it.
fn populated(file: &std::path::PathBuf) -> Connection {
    let conn = db::open(file).unwrap();
    principia_desk_lib::classroom::initialize(&conn).unwrap();
    conn.execute(
        "INSERT INTO classroom_schedule_slots(id, subject_id, hour, minute, created_at)
         VALUES (7, 'typescript', 18, 30, '2026-09-10T18:00:00')",
        [],
    )
    .unwrap();
    db::set_config(&conn, "onboarded", "1").unwrap();
    db::set_config(&conn, "escape_phrase", "a phrase long enough to mean it").unwrap();
    conn
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

fn document(archive: &Archive) -> String {
    serde_json::to_string(archive).unwrap()
}

#[test]
fn a_profile_survives_a_round_trip_through_an_archive() {
    let source_path = path("source");
    let source = populated(&source_path);
    let archive = portability::export(&source, "0.1.0").unwrap();
    let summary = portability::summarize(&archive);
    assert_eq!(summary.schedules, 1);
    assert!(summary.classes >= 1, "the seeded programs are carried too");
    assert!(summary.rows > 0);
    drop(source);

    // A different machine: a fresh database of the same schema version.
    let target_path = path("target");
    let mut target = db::open(&target_path).unwrap();
    assert_eq!(count(&target, "classroom_schedule_slots"), 0);
    assert!(db::get_config(&target, "escape_phrase").unwrap().is_none());

    let parsed = portability::read(&document(&archive)).unwrap();
    let written = portability::restore(&mut target, &parsed).unwrap();
    assert!(written > 0);
    assert_eq!(count(&target, "classroom_schedule_slots"), 1);
    assert_eq!(
        db::get_config(&target, "escape_phrase").unwrap().as_deref(),
        Some("a phrase long enough to mean it")
    );
    let hour: i64 = target
        .query_row(
            "SELECT hour FROM classroom_schedule_slots WHERE id = 7",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hour, 18);

    // Importing the same archive twice lands in the same place rather than
    // doubling the record.
    let again = portability::restore(&mut target, &parsed).unwrap();
    assert_eq!(again, written);
    assert_eq!(count(&target, "classroom_schedule_slots"), 1);
}

#[test]
fn an_import_replaces_the_profile_it_finds() {
    let source_path = path("replace-source");
    let source = populated(&source_path);
    let archive = portability::export(&source, "0.1.0").unwrap();
    drop(source);

    let target_path = path("replace-target");
    let mut target = db::open(&target_path).unwrap();
    principia_desk_lib::classroom::initialize(&target).unwrap();
    target
        .execute(
            "INSERT INTO classroom_schedule_slots(id, subject_id, hour, minute, created_at)
             VALUES (99, 'german', 7, 15, '2026-09-01T07:00:00')",
            [],
        )
        .unwrap();
    db::set_config(&target, "escape_phrase", "the phrase on this machine").unwrap();

    portability::restore(&mut target, &archive).unwrap();
    assert_eq!(count(&target, "classroom_schedule_slots"), 1);
    assert_eq!(
        target
            .query_row(
                "SELECT COUNT(*) FROM classroom_schedule_slots WHERE id = 99",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0,
        "the record that was here is replaced, not merged"
    );
    assert_eq!(
        db::get_config(&target, "escape_phrase").unwrap().as_deref(),
        Some("a phrase long enough to mean it")
    );
}

#[test]
fn a_damaged_or_foreign_file_is_refused_before_anything_is_written() {
    let source_path = path("refuse-source");
    let source = populated(&source_path);
    let archive = portability::export(&source, "0.1.0").unwrap();
    drop(source);

    // Not an archive at all.
    assert!(portability::read("{\"hello\":true}").is_err());
    assert!(portability::read("not json").is_err());

    // Edited after it was written: the checksum no longer describes it.
    let edited = document(&archive).replace("\"hour\",", "\"hour\",\"hour\",");
    assert!(portability::read(&edited).is_err());
    let mut tampered = archive.clone();
    tampered
        .tables
        .get_mut("config")
        .unwrap()
        .rows
        .push(vec![serde_json::json!("smuggled"), serde_json::json!("1")]);
    let message = portability::read(&document(&tampered))
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("damaged") || message.contains("checksum"),
        "the learner is told the file no longer matches itself: {message}"
    );

    // Written by a newer build of the app.
    let mut newer = archive.clone();
    newer.schema_version = portability::SCHEMA_VERSION + 1;
    newer.checksum = portability::read(&document(&archive)).unwrap().checksum;
    let message = portability::read(&document(&newer))
        .unwrap_err()
        .to_string();
    assert!(message.contains("newer version"), "{message}");

    // Written against a different database schema.
    let target_path = path("refuse-target");
    let mut target = db::open(&target_path).unwrap();
    let mut older = archive.clone();
    older.database_version = 1;
    let message = portability::restore(&mut target, &older)
        .unwrap_err()
        .to_string();
    assert!(message.contains("database version"), "{message}");
    assert_eq!(
        count(&target, "classroom_schedule_slots"),
        0,
        "a refused import writes nothing"
    );
}

#[test]
fn a_blob_and_every_column_type_survive_the_journey() {
    let source_path = path("types-source");
    let source = db::open(&source_path).unwrap();
    source
        .execute_batch(
            "CREATE TABLE shapes (i INTEGER, r REAL, t TEXT, b BLOB, n TEXT);
             INSERT INTO shapes VALUES (42, 1.5, 'Grüße', x'00ff10', NULL);",
        )
        .unwrap();
    let archive = portability::export(&source, "0.1.0").unwrap();
    drop(source);

    let target_path = path("types-target");
    let mut target = db::open(&target_path).unwrap();
    target
        .execute_batch("CREATE TABLE shapes (i INTEGER, r REAL, t TEXT, b BLOB, n TEXT)")
        .unwrap();
    portability::restore(
        &mut target,
        &portability::read(&document(&archive)).unwrap(),
    )
    .unwrap();
    let row: (i64, f64, String, Vec<u8>, Option<String>) = target
        .query_row("SELECT i, r, t, b, n FROM shapes", [], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap();
    assert_eq!(row.0, 42);
    assert_eq!(row.1, 1.5);
    assert_eq!(row.2, "Grüße");
    assert_eq!(row.3, vec![0x00, 0xff, 0x10]);
    assert_eq!(row.4, None);
}

#[test]
fn a_queued_job_belongs_to_the_machine_that_queued_it() {
    let source_path = path("jobs");
    let source = populated(&source_path);
    let archive = portability::export(&source, "0.1.0").unwrap();
    for table in ["generation_jobs", "study_preparation_jobs", "agent_calls"] {
        assert!(
            !archive.tables.contains_key(table),
            "{table} depends on this machine's provider, keys and files"
        );
    }
    assert!(
        !archive.tables.contains_key("schema_migrations"),
        "the ledger describes the database that receives the import"
    );
    assert!(archive.tables.contains_key("classroom_schedule_slots"));
}

#[test]
fn the_execution_log_keeps_lines_per_run_and_prunes_old_ones() {
    use principia_desk_lib::execution_log;
    let file = path("log");
    let conn = db::open(&file).unwrap();
    principia_desk_lib::classroom::initialize(&conn).unwrap();
    let now = chrono::Utc::now();
    execution_log::append(&conn, None, "desk booted", now).unwrap();
    execution_log::append(
        &conn,
        Some(("study-a", "typescript")),
        "Claude Code · opus · lesson",
        now,
    )
    .unwrap();
    execution_log::append(
        &conn,
        Some(("study-a", "typescript")),
        "finished in 41.0s",
        now + chrono::Duration::seconds(41),
    )
    .unwrap();
    execution_log::append(
        &conn,
        Some(("study-b", "german")),
        "Codex · default · lesson",
        now + chrono::Duration::seconds(60),
    )
    .unwrap();

    let runs = execution_log::runs(&conn, 10, None).unwrap();
    assert_eq!(runs.len(), 2, "lines with no run are not a run");
    assert_eq!(runs[0].activity, "lesson");
    assert_eq!(runs[0].run_id, "study-b", "newest first");
    assert_eq!(runs[1].lines, 2);
    assert_eq!(runs[1].label, "TypeScript", "the class label, not its id");
    assert_eq!(
        runs[1].outcome, "unknown",
        "no preparation job exists for a fixture run"
    );

    let lines = execution_log::lines(&conn, "study-a").unwrap();
    assert_eq!(
        lines.iter().map(|l| l.line.as_str()).collect::<Vec<_>>(),
        vec!["Claude Code · opus · lesson", "finished in 41.0s"]
    );
    assert_eq!(execution_log::recent(&conn, 2).unwrap().len(), 2);

    // Lines past retention go; recent ones stay.
    execution_log::append(
        &conn,
        Some(("study-old", "typescript")),
        "long ago",
        now - chrono::Duration::days(40),
    )
    .unwrap();
    assert_eq!(execution_log::prune(&conn, now).unwrap(), 1);
    assert_eq!(execution_log::runs(&conn, 10, None).unwrap().len(), 2);

    // A class builder run: running while the feed names it, then judged by
    // its last line.
    execution_log::append(
        &conn,
        Some(("draft:custom-rust", "custom-rust")),
        "class builder: asking claude (opus) to draft \"Rust\"",
        now + chrono::Duration::seconds(90),
    )
    .unwrap();
    let runs = execution_log::runs(&conn, 10, Some("draft:custom-rust")).unwrap();
    assert_eq!(
        (runs[0].activity.as_str(), runs[0].outcome.as_str()),
        ("draft", "running")
    );
    let runs = execution_log::runs(&conn, 10, None).unwrap();
    assert_eq!(runs[0].outcome, "unknown");
    execution_log::append(
        &conn,
        Some(("draft:custom-rust", "custom-rust")),
        "class builder: 12 topics drafted, 0 issue(s) against the validator",
        now + chrono::Duration::seconds(300),
    )
    .unwrap();
    assert_eq!(
        execution_log::runs(&conn, 10, None).unwrap()[0].outcome,
        "done"
    );
    execution_log::append(
        &conn,
        Some(("review:custom-rust", "custom-rust")),
        "class builder: the review failed: timed out",
        now + chrono::Duration::seconds(400),
    )
    .unwrap();
    let runs = execution_log::runs(&conn, 10, None).unwrap();
    assert_eq!(
        (runs[0].activity.as_str(), runs[0].outcome.as_str()),
        ("review", "failed")
    );
}
