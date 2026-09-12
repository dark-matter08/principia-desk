//! The study alarm: it stands while an appointment is due, a snooze holds it
//! for a fixed time and never dismisses it, and only two things outrank it.

use principia_desk_lib::{
    alarm,
    classroom::{self, ConfigureClassroomInput, UpsertClassroomSlotInput},
    db,
    domain::schedule::{self, Clock, LocalInstant, ZoneResolver},
    generator::Generator,
    language,
    state::AppState,
};
use rusqlite::Connection;
use std::sync::{atomic::AtomicBool, Arc, Mutex};

/// The release token lives in the shared temporary directory, which every
/// test in this binary sees, so these tests take turns.
static SERIAL: Mutex<()> = Mutex::new(());
fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A fixed zone, so a due appointment is due regardless of the machine.
struct Utc;
impl ZoneResolver for Utc {
    fn resolve(&self, local: chrono::NaiveDateTime) -> LocalInstant {
        LocalInstant::Single(local.and_utc())
    }
    fn name(&self) -> String {
        "UTC".into()
    }
}

fn app_state(dir: &std::path::Path, conn: Connection) -> AppState {
    let mut generator = Generator::new(
        "claude".into(),
        None,
        dir.join("scratch"),
        Arc::new(Mutex::new("opus".into())),
        Arc::new(Mutex::new("claude".into())),
        Arc::new(Mutex::new(String::new())),
        Default::default(),
    );
    generator.runner.database = Some(dir.join("principia.db"));
    AppState {
        db: db::Db(Mutex::new(conn)),
        generator,
        data_dir: dir.to_path_buf(),
        locked: AtomicBool::new(false),
        alarm_ringing: AtomicBool::new(false),
        alarm_for: Mutex::new(None),
        panel_height: Mutex::new(600.0),
        preparing_ahead: AtomicBool::new(false),
        debug_day: false,
        escape_failures: Mutex::new(vec![]),
        prev_muted: Mutex::new(None),
        frontend_ready: AtomicBool::new(true),
        class_start_gate: tokio::sync::Mutex::new(()),
        chat_threads: Mutex::new(Default::default()),
        focus: Default::default(),
    }
}

/// A desk with one active class whose study time is already in the past
/// today, so its appointment is due the moment it is materialised. The
/// clock here resolves in UTC, so "today" is the UTC date: between local
/// midnight and UTC midnight the local date is a day ahead, and a slot at
/// midnight of that day would not be due for another hour.
fn desk_with_due_class() -> (std::path::PathBuf, AppState) {
    let dir = std::env::temp_dir().join(format!(
        "principia-alarm-{}-{:016x}",
        std::process::id(),
        rand::random::<u64>()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::open(&dir.join("principia.db")).unwrap();
    db::seed_concepts(&conn, include_str!("../seed/concepts.json")).unwrap();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    language::initialize(&conn, &today).unwrap();
    classroom::initialize(&conn).unwrap();
    classroom::upsert_slot(
        &conn,
        &UpsertClassroomSlotInput {
            id: None,
            subject_id: "typescript".into(),
            hour: 0,
            minute: 0,
            weekdays: vec![1, 2, 3, 4, 5, 6, 7],
            enabled: true,
            durations: Default::default(),
            starts: Default::default(),
        },
    )
    .unwrap();
    classroom::configure_program(
        &conn,
        &ConfigureClassroomInput {
            subject_id: "typescript".into(),
            enabled: true,
            agent: "claude".into(),
            model: "opus".into(),
            custom_agent_bin: String::new(),
            session_minutes: 30,
            start_level: None,
            target_level: None,
            weekly_minutes: None,
        },
        &today,
    )
    .unwrap();
    // Midnight today has passed, so the appointment is due now.
    let clock = Clock {
        zone: &Utc,
        now: chrono::Utc::now(),
        today: chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d").unwrap(),
    };
    schedule::materialize(&conn, &clock, false).unwrap();
    (dir.clone(), app_state(&dir, conn))
}

#[test]
fn an_alarm_stands_for_a_due_appointment_until_the_lesson_starts() {
    let _turn = serial();
    let (_, state) = desk_with_due_class();
    let alarm = alarm::current(&state).expect("a due appointment rings");
    assert_eq!(alarm.course_id, "typescript");
    assert_eq!(alarm.snoozed_until, None);
    assert_eq!(alarm.queued, 0);

    // Starting the lesson claims the appointment, and the alarm has nothing
    // left to stand for.
    {
        let conn = state.db.0.lock().unwrap();
        schedule::claim(
            &conn,
            &alarm.occurrence_id,
            "typescript",
            "study-1",
            chrono::Utc::now(),
        )
        .unwrap();
    }
    assert_eq!(alarm::current(&state), None);
}

#[test]
fn a_snooze_holds_the_alarm_for_a_fixed_time_and_is_never_a_dismissal() {
    let _turn = serial();
    let (_, state) = desk_with_due_class();
    let alarm = alarm::current(&state).unwrap();

    // Only the offered lengths are accepted: a snooze cannot become "never".
    assert!(alarm::snooze(&state, &alarm.occurrence_id, 0).is_err());
    assert!(alarm::snooze(&state, &alarm.occurrence_id, 600).is_err());
    assert!(alarm::snooze(&state, "no-such-appointment", 5).is_err());

    let until = alarm::snooze(&state, &alarm.occurrence_id, 10).unwrap();
    let held = alarm::current(&state).expect("the appointment is still due");
    assert_eq!(held.occurrence_id, alarm.occurrence_id);
    assert_eq!(held.snoozed_until.as_deref(), Some(until.as_str()));
    let ends = chrono::DateTime::parse_from_rfc3339(&until).unwrap();
    let minutes = (ends.with_timezone(&chrono::Utc) - chrono::Utc::now()).num_minutes();
    assert!(
        (9..=10).contains(&minutes),
        "a ten-minute snooze ends in ten minutes, not later"
    );

    // A snooze that has passed no longer holds anything.
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(
            &conn,
            &format!("alarm_snooze:{}", alarm.occurrence_id),
            &(chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
        )
        .unwrap();
    }
    assert_eq!(alarm::current(&state).unwrap().snoozed_until, None);
}

#[test]
fn pausing_the_schedule_and_the_release_token_outrank_the_alarm() {
    let _turn = serial();
    let (dir, state) = desk_with_due_class();
    assert!(alarm::current(&state).is_some());
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "schedule_paused", "1").unwrap();
    }
    assert_eq!(
        alarm::current(&state),
        None,
        "a paused schedule owes nothing, so nothing rings"
    );
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "schedule_paused", "0").unwrap();
    }
    assert!(alarm::current(&state).is_some());
    // The release token is the floor of the safety ladder and silences this too.
    let token = std::env::temp_dir().join(principia_desk_lib::kiosk::UNLOCK_TOKEN);
    std::fs::write(&token, b"").unwrap();
    let silenced = alarm::current(&state);
    let _ = std::fs::remove_file(&token);
    assert_eq!(silenced, None, "a release token silences the alarm");
    let _ = std::fs::remove_dir_all(dir);
}

/// Publish a fixture lesson for a planned session, the way the tutor would.
fn publish(conn: &rusqlite::Connection, session: &principia_desk_lib::domain::sessions::Session) {
    use principia_desk_lib::{
        classroom::{StoredEngineeringLesson, StoredQuestion},
        domain::sessions::{self, PreparedLesson},
        subjects::engineering,
    };
    let now = chrono::Utc::now();
    let lease = sessions::claim_preparation(conn, &session.id, now, 60)
        .unwrap()
        .unwrap();
    let chosen = engineering::selection(session).unwrap();
    let stored = StoredEngineeringLesson {
        concept_id: chosen.concept_id,
        concept_title: chosen.title.clone(),
        category: chosen.category.clone(),
        title: format!("{} from first principles", chosen.title),
        markdown: "## The simple version\n\nA grounded fixture lesson.".into(),
        resources: vec![],
        review_notes: vec![],
        research_note: None,
        level: "standard".into(),
        questions: (1..=5)
            .map(|id| StoredQuestion {
                id,
                prompt: format!("Question {id}"),
                choices: vec!["right".into(), "wrong".into(), "also".into(), "no".into()],
                correct_index: 0,
                explanation: format!("Because of mechanism {id}."),
                section: "Core mechanics".into(),
                learning_objective: format!("Objective {id}"),
            })
            .collect(),
        exercise: None,
        source: "fixture".into(),
        path: None,
    };
    sessions::publish_preparation(
        conn,
        &lease,
        &PreparedLesson {
            title: stored.title.clone(),
            body: serde_json::to_value(&stored).unwrap(),
            provenance: serde_json::json!({"kind":"fixture"}),
        },
        now,
    )
    .unwrap();
}

#[test]
fn the_alarm_waits_for_the_lesson_and_rings_once_it_is_ready() {
    use principia_desk_lib::{readiness, subjects::engineering};
    let _turn = serial();
    let (_, state) = desk_with_due_class();
    let due = alarm::current(&state).unwrap();
    assert_eq!(
        due.readiness,
        readiness::Readiness::Preparing,
        "nothing is prepared yet"
    );
    assert!(
        !due.ringing(),
        "a due appointment without a lesson does not ring"
    );

    // The watcher wants this appointment prepared: it is due and has no lesson.
    let wanted = readiness::wanting_preparation(&state, chrono::Utc::now());
    assert_eq!(
        wanted,
        vec![(due.occurrence_id.clone(), "typescript".to_string())]
    );

    // Planning alone is still "preparing"; publishing makes it ready.
    let planned = {
        let conn = state.db.0.lock().unwrap();
        let program = principia_desk_lib::classroom::program_row(&conn, "typescript").unwrap();
        engineering::plan(
            &conn,
            &program,
            None,
            Some(due.occurrence_id.clone()),
            &state.today(),
            false,
        )
        .unwrap()
    };
    assert_eq!(
        alarm::current(&state).unwrap().readiness,
        readiness::Readiness::Preparing
    );
    assert!(
        readiness::wanting_preparation(&state, chrono::Utc::now()).is_empty(),
        "a planned lesson is not wanted twice"
    );
    {
        let conn = state.db.0.lock().unwrap();
        publish(&conn, &planned);
    }
    let ready = alarm::current(&state).unwrap();
    assert_eq!(ready.readiness, readiness::Readiness::Ready);
    assert!(ready.ringing(), "a ready lesson rings");

    // A failed preparation is reported, not rung, and Start retries it.
    let planned_again = {
        let conn = state.db.0.lock().unwrap();
        let program = principia_desk_lib::classroom::program_row(&conn, "typescript").unwrap();
        // Skip the ready lesson so a fresh one can be planned and then fail.
        engineering::skip(&conn, &planned.id).unwrap();
        // Skipping resolved the appointment; reopen the test with a new day's appointment.
        conn.execute("UPDATE schedule_occurrences SET disposition='due', session_ref=NULL, resolved_at=NULL WHERE id=?1", [&due.occurrence_id]).unwrap();
        let session = engineering::plan(
            &conn,
            &program,
            None,
            Some(due.occurrence_id.clone()),
            &state.today(),
            false,
        )
        .unwrap();
        let lease = principia_desk_lib::domain::sessions::claim_preparation(
            &conn,
            &session.id,
            chrono::Utc::now(),
            60,
        )
        .unwrap()
        .unwrap();
        principia_desk_lib::domain::sessions::fail_preparation(
            &conn,
            &lease,
            "provider unavailable",
            chrono::Utc::now(),
        )
        .unwrap();
        session
    };
    let failed = alarm::current(&state).unwrap();
    assert_eq!(failed.readiness, readiness::Readiness::Failed);
    assert_eq!(failed.error.as_deref(), Some("provider unavailable"));
    assert!(!failed.ringing());
    let _ = planned_again;
}

#[test]
fn a_lesson_started_by_hand_serves_the_appointment_that_comes_due_after_it() {
    use principia_desk_lib::{readiness, subjects::engineering};
    let _turn = serial();
    let (_, state) = desk_with_due_class();
    let due = alarm::current(&state).unwrap();

    // The learner opened a lesson by hand before the study time: it has no
    // appointment on it. Preparing another would be refused, so readiness
    // reports this one, and once it is open the alarm has nothing to ring for.
    let planned = {
        let conn = state.db.0.lock().unwrap();
        let program = principia_desk_lib::classroom::program_row(&conn, "typescript").unwrap();
        engineering::plan(&conn, &program, None, None, &state.today(), false).unwrap()
    };
    assert_eq!(
        engineering::selection(&planned).unwrap().occurrence_id,
        None
    );
    {
        let conn = state.db.0.lock().unwrap();
        let seen = readiness::for_occurrence(&conn, &due.occurrence_id, "typescript");
        assert_eq!(seen.session_id.as_deref(), Some(planned.id.0.as_str()));
        assert_eq!(seen.readiness, readiness::Readiness::Preparing);
    }
    assert!(
        readiness::wanting_preparation(&state, chrono::Utc::now()).is_empty(),
        "the class's own lesson is the one being prepared; no second one is wanted"
    );
    {
        let conn = state.db.0.lock().unwrap();
        publish(&conn, &planned);
    }
    assert!(
        alarm::current(&state).unwrap().ringing(),
        "ready by hand still rings for the due time"
    );
    {
        let conn = state.db.0.lock().unwrap();
        engineering::activate(&conn, &planned.id).unwrap();
    }
    assert_eq!(
        alarm::current(&state),
        None,
        "a lesson that is open is being studied; the appointment is served"
    );
}
