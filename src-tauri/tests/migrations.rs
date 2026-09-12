use principia_desk_lib::db;
use rusqlite::{types::Value, Connection};
use std::path::PathBuf;

static SERIAL: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn path() -> PathBuf {
    let n = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("principia-upgrade-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("learner.db")
}
fn backups(path: &std::path::Path) -> Vec<PathBuf> {
    std::fs::read_dir(path.parent().unwrap().join("backups"))
        .into_iter()
        .flatten()
        .map(|entry| entry.unwrap().path())
        .collect()
}
fn schema_version(conn: &Connection) -> u32 {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn rows(conn: &Connection, table: &str, columns: &[String], condition: &str) -> Vec<Vec<Value>> {
    let sql = format!(
        "SELECT {} FROM {} {condition} ORDER BY rowid",
        columns
            .iter()
            .map(|column| quote(column))
            .collect::<Vec<_>>()
            .join(","),
        quote(table)
    );
    conn.prepare(&sql)
        .unwrap()
        .query_map([], |row| (0..columns.len()).map(|i| row.get(i)).collect())
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap()
}

/// The schema version every fixture must arrive at.
const LATEST: u32 = 16;

#[test]
fn original_main_pr_head_and_intermediate_schemas_preserve_all_original_fields() {
    for (name, schema, classroom, unified) in [
        (
            "original-main",
            include_str!("fixtures/upgrades/original-main.sql"),
            false,
            false,
        ),
        (
            "pr-head",
            include_str!("fixtures/upgrades/pr-head.sql"),
            true,
            true,
        ),
        (
            "classroom-exercises",
            include_str!("fixtures/upgrades/classroom-exercises.sql"),
            true,
            false,
        ),
    ] {
        let path = path();
        let old = Connection::open(&path).unwrap();
        old.execute_batch(schema).unwrap();
        old.execute_batch(include_str!("fixtures/upgrades/common-records.sql"))
            .unwrap();
        if classroom {
            old.execute_batch(include_str!("fixtures/upgrades/classroom-records.sql"))
                .unwrap();
        }
        if unified {
            old.execute("INSERT INTO exercise_drafts (classroom_session_id, draft, completed, reflection, updated_at) VALUES (501, 'class draft with owner 501', 1, 'class reflection', '2026-09-08T11:15:00Z')", []).unwrap();
        }
        let tables = old.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([], |row| row.get::<_,String>(0)).unwrap().collect::<std::result::Result<Vec<_>,_>>().unwrap();
        let snapshots = tables
            .iter()
            .map(|table| {
                let columns = old
                    .prepare("SELECT name FROM pragma_table_info(?1)")
                    .unwrap()
                    .query_map([table], |row| row.get::<_, String>(0))
                    .unwrap()
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .unwrap();
                let content = rows(&old, table, &columns, "");
                (table.clone(), columns, content)
            })
            .collect::<Vec<_>>();
        drop(old);
        let conn = db::open(&path).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(schema_version(&conn), LATEST);
        for (table, columns, original) in snapshots {
            let condition = if table == "exercise_drafts" && !unified {
                "WHERE course_id IS NOT NULL"
            } else {
                ""
            };
            assert_eq!(
                rows(&conn, &table, &columns, condition),
                original,
                "{name}: changed original fields in {table}"
            );
        }
        assert_eq!(
            conn.query_row("SELECT focus FROM concepts WHERE id = 50", [], |row| row
                .get::<_, String>(
                0
            ))
            .unwrap(),
            "system-design"
        );
        assert_eq!(
            conn.query_row(
                "SELECT last_assessed_date FROM mastery WHERE concept_id = 50",
                [],
                |row| row.get::<_, Option<String>>(0)
            )
            .unwrap(),
            None,
            "reading history must not become invented assessment history"
        );
        if classroom {
            assert_eq!(
                db::get_exercise_draft(&conn, Some(501), None)
                    .unwrap()
                    .as_deref(),
                Some("course draft with owner 501")
            );
            assert_eq!(
                db::get_exercise_draft(&conn, None, Some(501))
                    .unwrap()
                    .as_deref(),
                Some("class draft with owner 501")
            );
        }
        assert!(!conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap());
        let backup_paths = backups(&path);
        assert_eq!(backup_paths.len(), 1);
        let snapshot = Connection::open(&backup_paths[0]).unwrap();
        assert_eq!(schema_version(&snapshot), 0);
        assert_eq!(
            snapshot
                .query_row("SELECT COUNT(*) FROM courses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        drop(snapshot);
        drop(conn);
        let reopened = db::open(&path).unwrap();
        assert_eq!(schema_version(&reopened), LATEST);
        assert_eq!(
            backups(&path),
            backup_paths,
            "ordinary opens must not rerun migrations or create backups"
        );
    }
}

#[test]
fn pre_upgrade_backup_includes_committed_wal_pages() {
    let path = path();
    let writer = Connection::open(&path).unwrap();
    writer.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE config(key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO config VALUES ('committed-in-wal', 'preserve me');").unwrap();
    assert!(
        std::fs::metadata(path.with_extension("db-wal"))
            .unwrap()
            .len()
            > 0
    );
    let conn = db::open(&path).unwrap();
    let snapshot = Connection::open(&backups(&path)[0]).unwrap();
    assert_eq!(
        db::get_config(&snapshot, "committed-in-wal")
            .unwrap()
            .as_deref(),
        Some("preserve me")
    );
    assert!(!snapshot
        .prepare("SELECT 1 FROM sqlite_schema WHERE name = 'schema_migrations'")
        .unwrap()
        .exists([])
        .unwrap());
    assert_eq!(schema_version(&conn), LATEST);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&backups(&path)[0])
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn edited_migration_history_and_newer_versions_are_rejected_without_mutation() {
    for mutation in [
        "UPDATE schema_migrations SET checksum = 'changed'",
        "PRAGMA user_version=99",
        "INSERT INTO schema_migrations VALUES (99, 'future', 'future', 'later', NULL)",
    ] {
        let path = path();
        let conn = db::open(&path).unwrap();
        conn.execute_batch(mutation).unwrap();
        let original_version = schema_version(&conn);
        drop(conn);
        assert!(db::open(&path).is_err());
        assert_eq!(
            schema_version(&Connection::open(&path).unwrap()),
            original_version
        );
        assert!(backups(&path).is_empty());
    }
}

#[test]
fn backup_failure_leaves_the_original_schema_unchanged() {
    let path = path();
    let original = Connection::open(&path).unwrap();
    original.execute_batch("CREATE TABLE config(key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO config VALUES ('retained', 'yes');").unwrap();
    std::fs::write(
        path.parent().unwrap().join("backups"),
        "a file blocks backup creation",
    )
    .unwrap();
    assert!(db::open(&path).is_err());
    assert_eq!(schema_version(&original), 0);
    assert!(!original
        .prepare("SELECT 1 FROM sqlite_schema WHERE name = 'schema_migrations'")
        .unwrap()
        .exists([])
        .unwrap());
    assert_eq!(
        db::get_config(&original, "retained").unwrap().as_deref(),
        Some("yes")
    );
}

#[test]
fn concurrent_opens_share_one_upgrade_and_one_pre_upgrade_snapshot() {
    let path = path();
    Connection::open(&path).unwrap().execute_batch("CREATE TABLE config(key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO config VALUES ('retained', 'yes');").unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let workers = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let conn = db::open(&path).unwrap();
                assert_eq!(schema_version(&conn), LATEST);
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(backups(&path).len(), 1);
}
