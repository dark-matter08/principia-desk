use principia_desk_lib::{
    classroom, db, language,
    progress::{self, ProgressQuery},
};
use rusqlite::{params, Connection};

fn fixture() -> Connection {
    let root = std::env::temp_dir().join(format!(
        "principia-progress-{:032x}",
        rand::random::<u128>()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let conn = db::open(&root.join("test.db")).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    language::initialize(&conn, "2026-09-10").unwrap();
    classroom::initialize(&conn).unwrap();
    conn
}
fn engineering(conn: &Connection, date: &str, status: &str, title: &str) -> i64 {
    conn.execute("INSERT INTO classroom_sessions(subject_id,session_date,status,title,payload_json,score,prompt_version,started_at) VALUES('linux-bash',?1,?2,?3,?4,0.75,'v1',?1)",params![date,status,title,serde_json::json!({"title":title,"markdown":format!("# {title}\n\nSaved engineering content.")}).to_string()]).unwrap();
    conn.last_insert_rowid()
}
#[test]
fn progress_counts_all_engines_without_counting_pending_or_duplicating_days() {
    let conn = fixture();
    conn.execute("INSERT INTO sessions(date,status,focus) VALUES('2026-09-09','completed','javascript'),('2026-09-10','pending','javascript')",[]).unwrap();
    engineering(&conn, "2026-09-10", "completed", "Shell navigation");
    engineering(&conn, "2026-09-10", "skipped", "Skipped practice");
    conn.execute("INSERT INTO language_sessions(language,session_date,level,unit_slug,status,lesson_json,started_at) VALUES('german','2026-09-10','A1','greetings','completed','{\"title\":\"Guten Tag\",\"markdown\":\"# Hallo\"}','2026-09-10')",[]).unwrap();
    let changes = conn.total_changes();
    let data = progress::read(&conn, "2026-09-10", &ProgressQuery::default()).unwrap();
    assert_eq!(
        conn.total_changes(),
        changes,
        "progress must remain read-only"
    );
    assert_eq!(data.classes.len(), 9);
    assert_eq!(data.completed_sessions, 3);
    assert_eq!(data.study_days, 2);
    assert_eq!(data.streak, 2);
    assert_eq!(data.history_total, 4);
    assert_eq!(data.activity.len(), 28);
    assert_eq!(data.activity.last().unwrap().completed, 2);
    assert!(data
        .history
        .iter()
        .any(|h| h.source == "language" && h.title == "Guten Tag"));
    let data = progress::read(
        &conn,
        "2026-09-10",
        &ProgressQuery {
            subject_id: Some("linux-bash".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(data.completed_sessions, 1);
    assert_eq!(data.history_total, 2);
    assert!(data.history.iter().all(|h| h.subject_id == "linux-bash"));
    assert_eq!(
        data.classes
            .iter()
            .find(|c| c.subject_id == "linux-bash")
            .unwrap()
            .completed_sessions,
        1
    );
}
#[test]
fn filtered_history_is_paginated_without_truncating_totals_or_summary() {
    let conn = fixture();
    for i in 0..19 {
        engineering(&conn, "2026-09-09", "completed", &format!("Pipes {i}"));
    }
    engineering(&conn, "2026-09-10", "in_progress", "Pending exercise");
    let query = ProgressQuery {
        search: "pipes".into(),
        status: Some("completed".into()),
        page: 1,
        ..Default::default()
    };
    let data = progress::read(&conn, "2026-09-10", &query).unwrap();
    assert_eq!(data.history_total, 19);
    assert_eq!(data.history.len(), 8);
    assert_eq!(data.page, 1);
    assert_eq!(data.completed_sessions, 19);
    assert_eq!(data.study_days, 1);
    assert_eq!(data.streak, 1);
    let last = progress::read(
        &conn,
        "2026-09-10",
        &ProgressQuery {
            page: i64::MAX,
            ..query
        },
    )
    .unwrap();
    assert_eq!(last.page, 2);
    assert_eq!(last.history.len(), 3);
    assert!(last.history.iter().all(|row| !data
        .history
        .iter()
        .any(|other| other.owner_id == row.owner_id)));
    let empty = progress::read(
        &conn,
        "2026-09-10",
        &ProgressQuery {
            search: "%".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        empty.history_total, 0,
        "search treats SQL wildcard characters literally"
    );
    assert_eq!(
        empty.completed_sessions, 19,
        "history search must not rewrite lifetime totals"
    );
}
#[test]
fn archived_lessons_resolve_source_and_stable_owner_without_mutating_learning() {
    let conn = fixture();
    let id = engineering(&conn, "2026-09-10", "completed", "Shell navigation");
    conn.execute("INSERT INTO language_sessions(id,language,session_date,level,unit_slug,status,lesson_json,started_at) VALUES(?1,'german','2026-09-10','A1','greetings','completed','{\"title\":\"Guten Tag\",\"markdown\":\"# Hallo\"}','2026-09-10')",[id]).unwrap();
    let concept: i64 = conn
        .query_row(
            "SELECT id FROM concepts WHERE focus='javascript' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute("INSERT INTO sessions(date,status,focus,concept_id) VALUES('2026-09-10','completed','javascript',?1)",[concept]).unwrap();
    conn.execute("INSERT INTO courses(session_date,concept_id,markdown,source,generated_at) VALUES('2026-09-10',?1,'# Earlier JavaScript','fallback','2026-09-10')",[concept]).unwrap();
    let course = conn.last_insert_rowid();
    let owner: String = conn
        .query_row(
            "SELECT session_id FROM primary_session_ids WHERE legacy_date='2026-09-10'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let changes = conn.total_changes();
    let engineering = progress::lesson(&conn, "classroom", &id.to_string())
        .unwrap()
        .unwrap();
    assert_eq!(engineering.classroom_session_id, Some(id));
    assert!(engineering.markdown.contains("Shell navigation"));
    let language = progress::lesson(&conn, "language", &id.to_string())
        .unwrap()
        .unwrap();
    assert_eq!(language.title, "Guten Tag");
    assert!(language.classroom_session_id.is_none());
    let primary = progress::lesson(&conn, "primary", &owner).unwrap().unwrap();
    assert_eq!(primary.course_id, Some(course));
    assert_eq!(primary.markdown, "# Earlier JavaScript");
    assert!(progress::lesson(&conn, "primary", "2026-09-10")
        .unwrap()
        .is_none());
    assert!(progress::lesson(&conn, "other", &id.to_string()).is_err());
    assert_eq!(conn.total_changes(), changes);
}
#[test]
fn empty_progress_has_zero_stats_and_stable_activity_dates() {
    let conn = fixture();
    let data = progress::read(&conn, "2026-01-01", &ProgressQuery::default()).unwrap();
    assert_eq!(data.completed_sessions, 0);
    assert_eq!(data.study_days, 0);
    assert_eq!(data.streak, 0);
    assert_eq!(data.activity.first().unwrap().date, "2025-12-05");
    assert_eq!(data.activity.last().unwrap().date, "2026-01-01");
    assert!(data.history.is_empty());
}
