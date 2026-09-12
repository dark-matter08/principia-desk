pub mod agents;
pub mod challenges;
pub mod classes;
pub mod custom;
use crate::db;
use crate::domain::assessments::RoundId;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, State};

type CmdResult<T> = Result<T, String>;
pub mod placement;
pub mod portability;
pub mod search;

#[tauri::command]
pub fn get_enrollment_options(
    state: State<'_, AppState>,
    course_id: String,
) -> CmdResult<crate::domain::enrollment::EnrollmentOptions> {
    let mut options = crate::domain::enrollment::options(&course_id).map_err(err)?;
    // A class you are enrolling in for the first time starts on the desk's
    // active tutor; class Settings is where a class gets its own afterwards.
    let custom = state.generator.current_custom_bin();
    options.default_configuration.tutor = crate::domain::enrollment::TutorPreference {
        provider: state.generator.current_agent(),
        model: state.generator.current_model(),
        custom_agent_bin: (!custom.trim().is_empty()).then_some(custom),
    };
    if let Some(configuration) =
        crate::domain::classes::current_configuration(&state.db.0.lock().unwrap(), &course_id)
            .map_err(err)?
    {
        options.default_configuration = configuration;
    }
    Ok(options)
}

#[tauri::command]
pub fn get_enrollment_draft(
    state: State<'_, AppState>,
    course_id: String,
) -> CmdResult<Option<crate::domain::enrollment::EnrollmentDraft>> {
    crate::domain::enrollment::draft_for_course(&state.db.0.lock().unwrap(), &course_id)
        .map_err(err)
}

#[tauri::command]
pub fn save_enrollment_draft(
    state: State<'_, AppState>,
    input: crate::domain::enrollment::SaveEnrollmentDraft,
) -> CmdResult<crate::domain::enrollment::EnrollmentDraft> {
    crate::domain::enrollment::save_draft(&state.db.0.lock().unwrap(), &input).map_err(err)
}

#[tauri::command]
pub fn get_catalog() -> CmdResult<Vec<crate::catalog::CourseDefinition>> {
    crate::catalog::validate()?;
    Ok(crate::catalog::all().into_iter().cloned().collect())
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[derive(Serialize)]
pub struct AppStateView {
    pub onboarded: bool,
    pub schedule_hour: u32,
    pub schedule_minute: u32,
    pub agent_ok: Option<bool>,
    pub debug_day: bool,
    /// A release token exists: every lock releases instantly. Surfaced so a
    /// leftover emergency file can't silently neuter enforcement.
    pub enforcement_disarmed: bool,
    /// Scheduler paused: no launchd agent, no owed sessions, until resumed.
    pub schedule_paused: bool,
    /// Kiosk strictness: 'advisory' | 'firm' | 'hard'.
    pub kiosk_level: String,
    /// Selected model identifier for the primary runner.
    pub model: String,
    /// Primary generation provider.
    pub agent: String,
    /// Binary path used when agent == 'custom'.
    pub custom_agent_bin: String,
    /// Today's chosen learning track, if the session has started with a focus.
    pub selected_focus: Option<String>,
    /// Whether a DeepSeek API key is available (env var or Keychain) — the
    /// key itself never leaves the Rust process.
    pub deepseek_key_configured: bool,
    /// The study alarm standing right now, if any. Only starting the lesson
    /// clears it; a snooze is reported with its end time.
    pub alarm: Option<crate::alarm::AlarmView>,
    /// Study blocks in progress today: appointments longer than one lesson
    /// that still have a next step.
    pub blocks: Vec<crate::classroom::BlockView>,
    /// Generic advisory classroom. Every subject owns its schedule, prompt
    /// profile, generation provider, progress, and same-day sessions.
    /// Languages are classroom subjects too; their CEFR engine lives behind
    /// the same panel.
    pub classroom_programs: Vec<crate::classroom::ClassroomProgramView>,
    pub classroom_slots: Vec<crate::classroom::ClassroomSlotView>,
    pub classroom_due_count: usize,
    pub active_classroom_sessions: Vec<crate::classroom::ActiveClassroomSessionView>,
    /// Today's durable appointments plus recent missed ones awaiting make-up.
    pub appointments: Vec<crate::classroom::AppointmentView>,
    /// The class session holding foreground enforcement, if any.
    pub focus: Option<crate::enforcement::FocusView>,
}

fn valid_agent(agent: &str) -> bool {
    crate::agents::RunnerId::parse(agent).is_some()
}

/// Switch the primary CLI agent (and the custom binary path when relevant).
/// Applies to the next generation; provider switching never creates a fallback.
#[tauri::command]
pub fn set_agent(
    state: State<'_, AppState>,
    agent: String,
    custom_bin: Option<String>,
) -> CmdResult<()> {
    if !valid_agent(&agent) {
        return Err(format!("unknown agent: {agent}"));
    }
    let custom = custom_bin.unwrap_or_default();
    if agent == "custom" && custom.trim().is_empty() {
        return Err("custom agent needs a binary path".into());
    }
    if agent == "custom" {
        let words = crate::agents::process::command_words(&custom).map_err(err)?;
        if crate::agents::process::resolve(&words[0]).is_none() {
            return Err("custom executable not found".into());
        }
    }
    let mut conn = state.db.0.lock().unwrap();
    let tx = conn.transaction().map_err(err)?;
    let runner = crate::agents::RunnerId::parse(&agent).ok_or("unknown runner")?;
    let prior_agent = db::get_config(&tx, "agent")
        .map_err(err)?
        .unwrap_or_else(|| "claude".into());
    let prior_model = db::get_config(&tx, "model")
        .map_err(err)?
        .unwrap_or_else(|| "opus".into());
    if let Some(prior) = crate::agents::RunnerId::parse(&prior_agent) {
        db::set_config(&tx, &format!("model_{}", prior.id()), &prior_model).map_err(err)?;
    }
    let model = db::get_config(&tx, &format!("model_{}", runner.id()))
        .map_err(err)?
        .unwrap_or_else(|| crate::agents::default_model(runner));
    db::set_config(&tx, "model", &model).map_err(err)?;
    db::set_config(&tx, "agent", &agent).map_err(err)?;
    db::set_config(&tx, "custom_agent_bin", custom.trim()).map_err(err)?;
    tx.commit().map_err(err)?;
    let mut current_agent = state.generator.agent.lock().unwrap();
    let mut current_model = state.generator.model.lock().unwrap();
    let mut current_custom = state.generator.custom_bin.lock().unwrap();
    *current_agent = agent;
    *current_model = model;
    *current_custom = custom.trim().into();
    Ok(())
}

fn valid_model(model: &str) -> bool {
    crate::agents::valid_model(model)
}

/// Change the course-generation model. Applies to the NEXT generation —
/// in-flight calls keep the model they started with.
#[tauri::command]
pub fn set_model(state: State<'_, AppState>, model: String) -> CmdResult<()> {
    if !valid_model(&model) {
        return Err(format!("unknown model: {model}"));
    }
    let mut conn = state.db.0.lock().unwrap();
    let tx = conn.transaction().map_err(err)?;
    let runner =
        crate::agents::RunnerId::parse(&state.generator.current_agent()).ok_or("unknown runner")?;
    db::set_config(&tx, "model", &model).map_err(err)?;
    db::set_config(&tx, &format!("model_{}", runner.id()), &model).map_err(err)?;
    tx.commit().map_err(err)?;
    *state.generator.model.lock().unwrap() = model;
    Ok(())
}

fn valid_kiosk_level(level: &str) -> bool {
    matches!(level, "advisory" | "firm" | "hard")
}

/// Change kiosk strictness. Refused while locked — strictness can't be
/// downgraded mid-session.
#[tauri::command]
pub fn set_kiosk_level(state: State<'_, AppState>, level: String) -> CmdResult<()> {
    if !valid_kiosk_level(&level) {
        return Err(format!("unknown kiosk level: {level}"));
    }
    if state.locked.load(Ordering::SeqCst) {
        return Err("cannot change enforcement during a locked session".into());
    }
    let conn = state.db.0.lock().unwrap();
    db::set_config(&conn, "kiosk_level", &level).map_err(err)
}

/// Called by the webview on boot. Until this fires, the kiosk refuses to
/// engage (a dead webview has no escape hatch).
#[tauri::command]
pub fn mark_frontend_ready(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    log::info!("frontend ready");
    state.frontend_ready.store(true, Ordering::SeqCst);
    // An active focused class session recovers its enforcement only now.
    crate::enforcement::restore(&app, &state);
    Ok(())
}

/// A failure in the interface, so it lands in the process log and the
/// execution feed instead of only in a toast nobody can copy.
#[tauri::command]
pub fn report_frontend_error(state: State<'_, AppState>, message: String) {
    let message: String = message.chars().take(2000).collect();
    log::error!("frontend: {message}");
    state.generator.feed.say(format!("interface: {message}"));
}

#[tauri::command]
pub fn get_app_state(state: State<'_, AppState>) -> CmdResult<AppStateView> {
    let (onboarded, hour, minute) = {
        let conn = state.db.0.lock().unwrap();
        (
            matches!(db::get_config(&conn, "onboarded"), Ok(Some(v)) if v == "1"),
            db::get_config(&conn, "schedule_hour")
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok())
                .unwrap_or(9),
            db::get_config(&conn, "schedule_minute")
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        )
    };
    let enforcement_disarmed = crate::kiosk::release_token().is_some();
    let (schedule_paused, kiosk_level, selected_focus) = {
        let conn = state.db.0.lock().unwrap();
        (
            matches!(db::get_config(&conn, "schedule_paused"), Ok(Some(v)) if v == "1"),
            db::get_config(&conn, "kiosk_level")
                .ok()
                .flatten()
                .unwrap_or_else(|| "hard".into()),
            crate::mastery::get_profile(&conn, "preferred_focus")
                .ok()
                .flatten()
                .filter(|focus| crate::focus::is_selectable(focus)),
        )
    };
    let (classroom_programs, classroom_slots, active_classroom_sessions, appointments) = {
        let conn = state.db.0.lock().unwrap();
        let appointments = if onboarded {
            crate::classroom::refresh_appointments(&conn, &state.today()).map_err(err)?
        } else {
            Vec::new()
        };
        (
            crate::classroom::program_views(&conn, &state.today()).map_err(err)?,
            crate::classroom::slot_views(&conn, &state.today(), state.debug_day).map_err(err)?,
            crate::classroom::active_sessions(&conn).map_err(err)?,
            appointments,
        )
    };
    let classroom_due_count = classroom_slots.iter().filter(|slot| slot.owed).count();
    Ok(AppStateView {
        onboarded,
        schedule_hour: hour,
        schedule_minute: minute,
        agent_ok: None,
        debug_day: state.debug_day,
        enforcement_disarmed,
        schedule_paused,
        kiosk_level,
        model: state.generator.current_model(),
        agent: state.generator.current_agent(),
        custom_agent_bin: state.generator.current_custom_bin(),
        selected_focus,
        deepseek_key_configured: deepseek_key_configured(),
        alarm: crate::alarm::current(&state),
        blocks: {
            let conn = state.db.0.lock().unwrap();
            crate::classroom::block_views(&conn, &state.today(), chrono::Utc::now()).map_err(err)?
        },
        classroom_programs,
        classroom_slots,
        classroom_due_count,
        active_classroom_sessions,
        appointments,
        focus: crate::enforcement::view(&state),
    })
}

/// Choose how future sessions of a class are enforced. Existing sessions keep
/// the policy they were planned with.
#[tauri::command]
pub fn set_class_focus_policy(
    app: AppHandle,
    state: State<'_, AppState>,
    subject_id: String,
    policy: crate::domain::enrollment::FocusPolicy,
) -> CmdResult<crate::classroom::ClassroomProgramView> {
    let view = {
        let conn = state.db.0.lock().unwrap();
        if crate::domain::classes::current_path(&conn, subject_id.trim())
            .map_err(err)?
            .is_none()
        {
            return Err(NO_STARTING_POINT.into());
        }
        crate::domain::classes::set_focus_policy(&conn, subject_id.trim(), policy, &state.today())
            .map_err(err)?;
        crate::classroom::program_view(&conn, subject_id.trim(), &state.today()).map_err(err)?
    };
    let _ = app.emit("classroom:state", &view);
    crate::alarm::evaluate(&app);
    Ok(view)
}

/// Skip an open appointment without starting a session. It is consumed once.
/// Move one unfired appointment to another time today. The in-app watcher and
/// the launch agent both honour the new time.
#[tauri::command]
pub fn reschedule_appointment(
    app: AppHandle,
    state: State<'_, AppState>,
    occurrence_id: String,
    local_time: String,
) -> CmdResult<crate::classroom::AppointmentView> {
    let view = {
        let conn = state.db.0.lock().unwrap();
        let moved = crate::domain::schedule::reschedule(
            &conn,
            &occurrence_id,
            &local_time,
            &crate::domain::schedule::SystemZone,
            chrono::Utc::now(),
        )
        .map_err(err)?;
        crate::classroom::appointment_view(&conn, moved).map_err(err)?
    };
    refresh_os_schedule(&state)?;
    let _ = app.emit("classroom:state", &view);
    Ok(view)
}

/// Bring the desk window forward from the menu bar panel.
#[tauri::command]
pub fn show_desk(app: AppHandle) {
    crate::tray::hide_panel(&app);
    crate::tray::show_window(&app);
}

/// Start the due class from the menu bar panel.
#[tauri::command]
pub fn start_from_tray(app: AppHandle, occurrence_id: String) -> CmdResult<()> {
    crate::tray::start_from_outside(&app, &occurrence_id)
}

/// Quit from the menu bar panel; refused while an alarm rings or a session holds the desk.
#[tauri::command]
pub fn quit_desk(app: AppHandle) -> CmdResult<()> {
    crate::tray::quit(&app)
}

/// Close the menu bar panel without doing anything.
#[tauri::command]
pub fn hide_tray_panel(app: AppHandle) {
    crate::tray::hide_panel(&app);
}

/// The panel measured its card; fit the window to it.
#[tauri::command]
pub fn size_tray_panel(app: AppHandle, height: f64) {
    crate::tray::size_panel(&app, height);
}

/// Runs the tutor made while preparing lessons, newest first.
#[tauri::command]
pub fn list_execution_runs(
    state: State<'_, AppState>,
) -> CmdResult<Vec<crate::execution_log::RunSummary>> {
    let conn = state.db.0.lock().unwrap();
    let current = state.generator.feed.current().map(|(run, _)| run);
    crate::execution_log::runs(&conn, 200, current.as_deref()).map_err(err)
}

/// Every runner line of one run, oldest first.
#[tauri::command]
pub fn get_execution_log(
    state: State<'_, AppState>,
    run_id: String,
) -> CmdResult<Vec<crate::execution_log::LogLine>> {
    let conn = state.db.0.lock().unwrap();
    crate::execution_log::lines(&conn, &run_id).map_err(err)
}

/// The latest runner lines regardless of run, for a live tail.
#[tauri::command]
pub fn get_recent_execution_log(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> CmdResult<Vec<crate::execution_log::LogLine>> {
    let conn = state.db.0.lock().unwrap();
    crate::execution_log::recent(&conn, limit.unwrap_or(300).clamp(1, 2_000)).map_err(err)
}

/// Hold the alarm for a fixed number of minutes. It rings again afterwards;
/// only starting the lesson ends it.
#[tauri::command]
pub fn snooze_alarm(
    app: AppHandle,
    state: State<'_, AppState>,
    occurrence_id: String,
    minutes: u32,
) -> CmdResult<String> {
    let until = crate::alarm::snooze(&state, &occurrence_id, minutes)?;
    crate::alarm::evaluate(&app);
    Ok(until)
}

#[tauri::command]
pub fn skip_appointment(
    app: AppHandle,
    state: State<'_, AppState>,
    occurrence_id: String,
) -> CmdResult<crate::classroom::AppointmentView> {
    let view = {
        let conn = state.db.0.lock().unwrap();
        let skipped = crate::domain::schedule::skip(&conn, &occurrence_id, chrono::Utc::now())
            .map_err(err)?;
        crate::classroom::appointment_view(&conn, skipped).map_err(err)?
    };
    let _ = app.emit("classroom:state", &view);
    Ok(view)
}

/// Appointment history of one class, newest first.
#[tauri::command]
pub fn get_class_appointments(
    state: State<'_, AppState>,
    subject_id: String,
) -> CmdResult<Vec<crate::classroom::AppointmentView>> {
    let conn = state.db.0.lock().unwrap();
    crate::classroom::appointment_history(&conn, subject_id.trim()).map_err(err)
}

/// A newly planned session starts its appointment; a resumed session that was
/// planned for another appointment leaves this one open.
fn claim_appointment(
    conn: &rusqlite::Connection,
    appointment: &Option<crate::domain::schedule::Occurrence>,
    course_id: &str,
    session_id: &str,
) -> CmdResult<()> {
    let Some(found) = appointment else {
        return Ok(());
    };
    let planned_for: Option<String> = conn
        .query_row(
            "SELECT json_extract(context_json,'$.selection.occurrence_id') FROM study_sessions WHERE id=?1",
            [session_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    // A lesson planned for another appointment (a make-up, say) is not this
    // one's; a lesson planned for none, such as one started by hand before the
    // appointment came due, is exactly the lesson the appointment opens.
    if planned_for
        .as_deref()
        .is_some_and(|planned| planned != found.id.as_str())
    {
        return Ok(());
    }
    if found.disposition == "started" {
        crate::domain::schedule::continue_block(
            conn,
            &found.id,
            course_id,
            &format!("study:{session_id}"),
            chrono::Utc::now(),
        )
        .map_err(err)?;
        return Ok(());
    }
    crate::domain::schedule::claim(
        conn,
        &found.id,
        course_id,
        &format!("study:{session_id}"),
        chrono::Utc::now(),
    )
    .map_err(err)?;
    Ok(())
}

/// End a study block before its time is spent: completed when a lesson
/// finished, skipped when none did.
#[tauri::command]
pub fn end_block(
    app: AppHandle,
    state: State<'_, AppState>,
    occurrence_id: String,
) -> CmdResult<crate::domain::schedule::Occurrence> {
    let ended = {
        let conn = state.db.0.lock().unwrap();
        crate::domain::schedule::end_block(&conn, &occurrence_id, chrono::Utc::now())
            .map_err(err)?
    };
    let _ = app.emit("classroom:state", serde_json::json!({ "refresh": true }));
    crate::alarm::evaluate(&app);
    Ok(ended)
}

fn refresh_os_schedule(state: &AppState) -> CmdResult<()> {
    if state.debug_day {
        return Ok(());
    }
    let times = {
        let conn = state.db.0.lock().unwrap();
        if matches!(db::get_config(&conn, "schedule_paused"), Ok(Some(value)) if value == "1") {
            Vec::new()
        } else {
            // Rule times plus today's unfired appointments, so a moved
            // appointment also wakes the app at the operating-system level.
            let mut times = crate::classroom::all_schedule_times(&conn).map_err(err)?;
            for time in
                crate::domain::schedule::unfired_times_today(&conn, &state.today()).map_err(err)?
            {
                if !times.contains(&time) {
                    times.push(time);
                }
            }
            times
        }
    };
    crate::scheduler::install_many(&times)
}

fn deepseek_key_configured() -> bool {
    std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some()
        || crate::keychain::has_secret("deepseek")
}

/// Store (or clear, with an empty string) the DeepSeek API key. Persisted
/// in the macOS Keychain, not the app database — see `keychain.rs`.
#[tauri::command]
pub fn set_deepseek_api_key(key: String) -> CmdResult<()> {
    crate::keychain::set_secret("deepseek", &key)
}

/// Pause all class appointments. Existing learning records remain resumable.
#[tauri::command]
pub fn pause_schedule(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "schedule_paused", "1").map_err(err)?;
    }
    if !state.debug_day {
        crate::scheduler::uninstall()?;
    }
    let _ = app.emit("classroom:state", serde_json::json!({ "refresh": true }));
    crate::alarm::evaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn resume_schedule(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "schedule_paused", "0").map_err(err)?;
    }
    refresh_os_schedule(&state)?;
    let _ = app.emit("classroom:state", serde_json::json!({ "refresh": true }));
    crate::alarm::evaluate(&app);
    Ok(())
}

/// Ping whichever agent is currently selected (optionally an explicit one,
/// so the setup wizard can test a choice before saving it).
#[tauri::command]
pub async fn check_agent(
    state: State<'_, AppState>,
    agent: Option<String>,
    custom_bin: Option<String>,
) -> CmdResult<bool> {
    Ok(agents::test_connection(&state, agent, custom_bin, None)
        .await?
        .ok)
}

#[derive(Deserialize)]
pub struct SetupInput {
    pub escape_phrase: String,
    #[serde(default)]
    pub kiosk_level: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub custom_agent_bin: Option<String>,
}

#[tauri::command]
pub async fn complete_setup(
    app: AppHandle,
    state: State<'_, AppState>,
    input: SetupInput,
) -> CmdResult<AppStateView> {
    if input.escape_phrase.trim().len() < 40 {
        return Err("escape phrase must be at least 40 characters".into());
    }
    {
        let conn = state.db.0.lock().unwrap();
        db::set_config(&conn, "escape_phrase", input.escape_phrase.trim()).map_err(err)?;
        let level = input.kiosk_level.as_deref().unwrap_or("hard");
        if !valid_kiosk_level(level) {
            return Err(format!("unknown kiosk level: {level}"));
        }
        db::set_config(&conn, "kiosk_level", level).map_err(err)?;
        let model = input.model.as_deref().unwrap_or("opus");
        if !valid_model(model) {
            return Err(format!("unknown model: {model}"));
        }
        db::set_config(&conn, "model", model).map_err(err)?;
        *state.generator.model.lock().unwrap() = model.to_string();
        let agent = input.agent.as_deref().unwrap_or("claude");
        if !valid_agent(agent) {
            return Err(format!("unknown agent: {agent}"));
        }
        let custom = input.custom_agent_bin.clone().unwrap_or_default();
        if agent == "custom" && custom.trim().is_empty() {
            return Err("custom agent needs a binary path".into());
        }
        db::set_config(&conn, "agent", agent).map_err(err)?;
        db::set_config(&conn, "custom_agent_bin", custom.trim()).map_err(err)?;
        *state.generator.agent.lock().unwrap() = agent.to_string();
        *state.generator.custom_bin.lock().unwrap() = custom.trim().to_string();
        db::set_config(&conn, "onboarded", "1").map_err(err)?;
        // Day-1 content generates after the student picks a focus at session start.
    }
    refresh_os_schedule(&state)?;
    let _ = app.emit("classroom:state", serde_json::json!({ "refresh": true }));
    get_app_state(state)
}

#[tauri::command]
pub fn get_curriculum_map(
    state: State<'_, AppState>,
    focus: String,
) -> CmdResult<crate::classroom::CurriculumMapView> {
    let conn = state.db.0.lock().unwrap();
    crate::classroom::curriculum_map(&conn, &focus).map_err(err)
}

#[tauri::command]
pub fn configure_classroom_program(
    app: AppHandle,
    state: State<'_, AppState>,
    input: crate::classroom::ConfigureClassroomInput,
) -> CmdResult<crate::classroom::ClassroomProgramView> {
    {
        let conn = state.db.0.lock().unwrap();
        // A class activates only once the learner has said where it starts.
        // A default path would decide that for them, and a lesson pitched at
        // the wrong level is the result.
        let turning_on = input.enabled
            && !crate::classroom::program_row(&conn, &input.subject_id)
                .map_err(err)?
                .enabled;
        if turning_on
            && crate::domain::classes::current_path(&conn, &input.subject_id)
                .map_err(err)?
                .is_none()
        {
            return Err(NO_STARTING_POINT.into());
        }
        crate::classroom::configure_program(&conn, &input, &state.today()).map_err(err)?;
    }
    refresh_os_schedule(&state)?;
    let view = {
        let conn = state.db.0.lock().unwrap();
        crate::classroom::program_view(&conn, &input.subject_id, &state.today()).map_err(err)?
    };
    let _ = app.emit("classroom:state", &view);
    Ok(view)
}

/// The refusal every path into an unenrolled class shares.
pub const NO_STARTING_POINT: &str = "Choose this class's starting point first: start from scratch, find your level, or pick a stage. Then activate it.";

#[tauri::command]
pub fn upsert_classroom_slot(
    app: AppHandle,
    state: State<'_, AppState>,
    input: crate::classroom::UpsertClassroomSlotInput,
) -> CmdResult<Vec<crate::classroom::ClassroomSlotView>> {
    {
        let conn = state.db.0.lock().unwrap();
        crate::classroom::upsert_slot(&conn, &input).map_err(err)?;
    }
    refresh_os_schedule(&state)?;
    let slots = {
        let conn = state.db.0.lock().unwrap();
        crate::classroom::slot_views(&conn, &state.today(), state.debug_day).map_err(err)?
    };
    let _ = app.emit("classroom:state", &slots);
    Ok(slots)
}

#[tauri::command]
pub fn delete_classroom_slot(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> CmdResult<Vec<crate::classroom::ClassroomSlotView>> {
    {
        let conn = state.db.0.lock().unwrap();
        crate::classroom::delete_slot(&conn, id).map_err(err)?;
    }
    refresh_os_schedule(&state)?;
    let slots = {
        let conn = state.db.0.lock().unwrap();
        crate::classroom::slot_views(&conn, &state.today(), state.debug_day).map_err(err)?
    };
    let _ = app.emit("classroom:state", &slots);
    Ok(slots)
}

/// Preview (`commit: false`) or persist (`commit: true`) a schedule derived
/// from a learner's stated goal, weekly-minutes target, and availability
/// windows. Additive to `upsert_classroom_slot` — committing only replaces
/// this subject's previously *planned* slots, never hand-placed ones.
#[tauri::command]
pub fn plan_classroom_schedule(
    app: AppHandle,
    state: State<'_, AppState>,
    input: crate::classroom::PlanClassroomScheduleInput,
) -> CmdResult<crate::classroom::ClassroomPlanView> {
    let commit = input.commit;
    let plan = {
        let conn = state.db.0.lock().unwrap();
        crate::classroom::plan_schedule(&conn, &input, &state.today()).map_err(err)?
    };
    if commit {
        refresh_os_schedule(&state)?;
        let _ = app.emit("classroom:state", &plan);
    }
    Ok(plan)
}

/// Route a classroom start to the subject's isolated engine. Advisory classes
/// never engage the kiosk or mutate the primary frontend session row.
/// `revisit` is the opt-in path for re-serving a completed module.
#[tauri::command]
pub async fn start_classroom_session(
    app: AppHandle,
    state: State<'_, AppState>,
    subject_id: String,
    slot_id: Option<i64>,
    revisit: Option<bool>,
    occurrence_id: Option<String>,
) -> CmdResult<serde_json::Value> {
    // A lesson may already be preparing ahead of its appointment; a Start
    // waits for it and then opens the ready lesson at once.
    let _start = state.class_start_gate.lock().await;
    let revisit = revisit.unwrap_or(false);
    let spec = crate::classroom::subject(subject_id.trim()).map_err(err)?;
    if let Some(holder) = state.focus.holder() {
        if holder.course_id != spec.id {
            return Err(format!(
                "A focused {} session holds the desk. Finish it or use the escape hatch first.",
                crate::focus::label(&holder.course_id)
            ));
        }
    }
    // Resolve the appointment this start serves: an explicit one (including a
    // missed make-up) or today's appointment of the chosen rule.
    let appointment = {
        let conn = state.db.0.lock().unwrap();
        if !crate::classroom::has_enabled_schedule(&conn, spec.id).map_err(err)? {
            return Err("Add a study time in this class's Schedule tab before starting it.".into());
        }
        let today = state.today();
        let _ = crate::classroom::refresh_appointments(&conn, &today);
        let found = match (&occurrence_id, slot_id) {
            (Some(id), _) => Some(crate::domain::schedule::get(&conn, id).map_err(err)?),
            (None, Some(rule)) => {
                crate::domain::schedule::today_for_rule(&conn, rule, &today).map_err(err)?
            }
            (None, None) => None,
        };
        if let Some(found) = &found {
            if found.course_id != spec.id {
                return Err("This appointment belongs to another class.".into());
            }
            let continuing = found.disposition == "started"
                && crate::domain::schedule::block_progress(&conn, &found.id, chrono::Utc::now())
                    .map_err(err)?
                    .is_some_and(|block| block.next == crate::domain::schedule::BlockStep::Topic);
            if found.consumed() && !continuing {
                return Err("This appointment was already started or resolved.".into());
            }
        }
        found
    };
    let appointment_id = appointment.as_ref().map(|found| found.id.clone());
    // Inside a block the rule's once-per-day guard has already been satisfied
    // by the first lesson; the block itself decides whether another fits.
    let slot_id = if appointment
        .as_ref()
        .is_some_and(|a| a.disposition == "started")
    {
        None
    } else {
        slot_id
    };
    let value = match spec.kind {
        crate::classroom::SubjectKind::Language => {
            // A legacy in-progress row resumes as it was; every new lesson runs
            // on the shared study runtime with the curated seed enriched once.
            let (legacy, planned) = {
                let conn = state.db.0.lock().unwrap();
                let legacy = crate::language::active_session_for(&conn, spec.id).map_err(err)?;
                if legacy.is_some() {
                    (legacy, None)
                } else {
                    let program = crate::classroom::program_row(&conn, spec.id).map_err(err)?;
                    let planned = crate::subjects::language::plan(
                        &conn,
                        &program,
                        slot_id,
                        appointment_id.clone(),
                        &state.today(),
                        revisit,
                    )?;
                    claim_appointment(&conn, &appointment, spec.id, &planned.id.0)?;
                    (None, Some(planned))
                }
            };
            let lesson = match (legacy, planned) {
                (Some(lesson), _) => lesson,
                (None, Some(planned)) => {
                    let _ = app.emit(
                        "classroom:state",
                        serde_json::json!({ "planned": planned.id }),
                    );
                    let prepared = {
                        let _run = state.generator.feed.begin(&planned.id.0, spec.id);
                        crate::subjects::language::prepare(&state, &planned.id).await
                    };
                    prepared?;
                    crate::enforcement::activate(&app, &state, &planned.id)?;
                    let conn = state.db.0.lock().unwrap();
                    crate::subjects::language::view(&conn, &planned.id)?
                        .ok_or("The prepared lesson could not be read.")?
                }
                (None, None) => unreachable!(),
            };
            serde_json::json!({ "kind": "language", "lesson": lesson })
        }
        crate::classroom::SubjectKind::Engineering => {
            // A legacy in-progress row finishes through the compatibility path;
            // every new lesson runs on the shared study runtime.
            let (legacy, planned) = {
                let conn = state.db.0.lock().unwrap();
                let legacy =
                    crate::classroom::active_engineering_session(&conn, spec.id).map_err(err)?;
                if legacy.is_some() {
                    (legacy, None)
                } else {
                    let program = crate::classroom::program_row(&conn, spec.id).map_err(err)?;
                    let planned = crate::subjects::engineering::plan(
                        &conn,
                        &program,
                        slot_id,
                        appointment_id.clone(),
                        &state.today(),
                        revisit,
                    )?;
                    claim_appointment(&conn, &appointment, spec.id, &planned.id.0)?;
                    (None, Some(planned))
                }
            };
            let lesson = match (legacy, planned) {
                (Some(lesson), _) => lesson,
                (None, Some(planned)) => {
                    let _ = app.emit(
                        "classroom:state",
                        serde_json::json!({ "planned": planned.id }),
                    );
                    let prepared = {
                        let _run = state.generator.feed.begin(&planned.id.0, spec.id);
                        crate::subjects::engineering::prepare(&state, &planned.id).await
                    };
                    prepared?;
                    crate::enforcement::activate(&app, &state, &planned.id)?;
                    let conn = state.db.0.lock().unwrap();
                    crate::subjects::engineering::view(&conn, &planned.id)?
                        .ok_or("The prepared lesson could not be read.")?
                }
                (None, None) => unreachable!(),
            };
            serde_json::json!({ "kind": "engineering", "lesson": lesson })
        }
    };
    let _ = app.emit("classroom:state", &value);
    crate::alarm::evaluate(&app);
    Ok(value)
}

/// Saved work for a shared-runtime lesson: stage, reading position and editor
/// fields. Feedback is only reached through the knowledge check. Cooperating
/// widgets of one open lesson (reading tracker, exercise workspace) may omit the
/// expected revision to merge their keys into the latest checkpoint.
#[derive(Deserialize)]
pub struct ClassLessonWorkInput {
    pub session_id: crate::domain::sessions::SessionId,
    #[serde(default)]
    pub expected_revision: Option<u32>,
    #[serde(default)]
    pub stage: Option<crate::domain::sessions::Stage>,
    #[serde(default)]
    pub reading: Option<crate::domain::sessions::ReadingPosition>,
    #[serde(default)]
    pub work: std::collections::BTreeMap<String, serde_json::Value>,
}

#[tauri::command]
pub fn save_class_lesson_work(
    state: State<'_, AppState>,
    input: ClassLessonWorkInput,
) -> CmdResult<crate::domain::sessions::Checkpoint> {
    let conn = state.db.0.lock().unwrap();
    let expected = match input.expected_revision {
        Some(revision) => revision,
        None => crate::subjects::engineering::checkpoint_revision(&conn, &input.session_id)?,
    };
    crate::subjects::engineering::patch_work(
        &conn,
        &input.session_id,
        expected,
        input.stage,
        input.reading,
        input.work,
    )
}

#[tauri::command]
pub fn save_class_check_answer(
    state: State<'_, AppState>,
    session_id: crate::domain::sessions::SessionId,
    round_id: crate::domain::assessments::RoundId,
    expected_revision: u32,
    question_id: usize,
    choice: Option<usize>,
) -> CmdResult<crate::classroom::CheckView> {
    let conn = state.db.0.lock().unwrap();
    crate::subjects::engineering::save_answer(
        &conn,
        &session_id,
        &round_id,
        expected_revision,
        question_id,
        choice,
    )
}

#[tauri::command]
pub fn submit_class_check(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: crate::domain::sessions::SessionId,
    round_id: crate::domain::assessments::RoundId,
    expected_revision: u32,
    reflection: String,
) -> CmdResult<crate::classroom::EngineeringSessionResult> {
    let result = {
        let conn = state.db.0.lock().unwrap();
        crate::subjects::engineering::submit(
            &conn,
            &session_id,
            &round_id,
            expected_revision,
            &reflection,
            &state.today(),
        )?
    };
    crate::enforcement::release(&app, &state, &session_id.0);
    state.clear_chat_threads();
    let _ = app.emit("classroom:state", &result);
    Ok(result)
}

#[tauri::command]
pub fn submit_class_language_check(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: crate::domain::sessions::SessionId,
    round_id: crate::domain::assessments::RoundId,
    expected_revision: u32,
    input: crate::subjects::language::LanguageCheckInput,
) -> CmdResult<serde_json::Value> {
    let result = {
        let conn = state.db.0.lock().unwrap();
        crate::subjects::language::submit(
            &conn,
            &session_id,
            &round_id,
            expected_revision,
            &input,
            &state.today(),
        )?
    };
    crate::enforcement::release(&app, &state, &session_id.0);
    state.clear_chat_threads();
    let _ = app.emit("classroom:state", &result);
    Ok(result)
}

/// Leave a lesson open for later without losing its saved work.
#[tauri::command]
pub fn pause_class_lesson(
    state: State<'_, AppState>,
    session_id: crate::domain::sessions::SessionId,
) -> CmdResult<()> {
    if state
        .focus
        .holder()
        .is_some_and(|holder| holder.session_id == session_id.0)
    {
        return Err(
            "This focused session cannot be paused. Finish its check, or use the escape hatch."
                .into(),
        );
    }
    let conn = state.db.0.lock().unwrap();
    crate::subjects::engineering::pause(&conn, &session_id).map(|_| ())
}

/// Discard a lesson without credit; its content and work stay in history.
#[tauri::command]
pub fn skip_class_lesson(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: crate::domain::sessions::SessionId,
) -> CmdResult<()> {
    {
        let conn = state.db.0.lock().unwrap();
        crate::subjects::engineering::skip(&conn, &session_id)?;
    }
    crate::enforcement::release(&app, &state, &session_id.0);
    state.clear_chat_threads();
    let _ = app.emit(
        "classroom:state",
        serde_json::json!({ "skipped": session_id }),
    );
    Ok(())
}

/// Start (or resume) a delayed-retrieval session for the class's most overdue
/// topic. Prepared from bundled material without a provider; activation goes
/// through the focus coordinator like any lesson.
#[tauri::command]
pub fn start_class_review(
    app: AppHandle,
    state: State<'_, AppState>,
    subject_id: String,
    occurrence_id: Option<String>,
) -> CmdResult<serde_json::Value> {
    let spec = crate::classroom::subject(subject_id.trim()).map_err(err)?;
    if spec.kind == crate::catalog::SubjectKind::Language {
        return Err("Reviews are available for engineering classes.".into());
    }
    if let Some(holder) = state.focus.holder() {
        if holder.course_id != spec.id {
            return Err(format!(
                "A focused {} session holds the desk. Finish it or use the escape hatch first.",
                crate::focus::label(&holder.course_id)
            ));
        }
    }
    let planned = {
        let conn = state.db.0.lock().unwrap();
        let program = crate::classroom::program_row(&conn, spec.id)?;
        let session = crate::subjects::engineering::plan_review(&conn, &program, &state.today())?;
        crate::subjects::engineering::prepare_review(&conn, &session.id)?;
        if let Some(occurrence) = occurrence_id.as_deref() {
            // The tail of a block: the review's end resolves the appointment.
            crate::domain::schedule::continue_block(
                &conn,
                occurrence,
                spec.id,
                &format!("study:{}", session.id.0),
                chrono::Utc::now(),
            )
            .map_err(err)?;
        }
        session.id
    };
    crate::enforcement::activate(&app, &state, &planned)?;
    let conn = state.db.0.lock().unwrap();
    let lesson = crate::subjects::engineering::view(&conn, &planned)?
        .ok_or("The review could not be read.")?;
    let _ = app.emit("classroom:state", serde_json::json!({ "review": planned }));
    Ok(serde_json::json!({ "kind": "engineering", "lesson": lesson }))
}

#[tauri::command]
pub fn resume_classroom_session(
    app: AppHandle,
    state: State<'_, AppState>,
    subject_id: String,
) -> CmdResult<Option<serde_json::Value>> {
    let spec = crate::classroom::subject(subject_id.trim()).map_err(err)?;
    match spec.kind {
        crate::classroom::SubjectKind::Language => {
            let conn = state.db.0.lock().unwrap();
            if let Some(lesson) =
                crate::language::active_session_for(&conn, spec.id).map_err(err)?
            {
                return Ok(Some(
                    serde_json::json!({ "kind": "language", "lesson": lesson }),
                ));
            }
            let Some(session) = crate::subjects::language::resumable(&conn, spec.id)? else {
                return Ok(None);
            };
            if !matches!(
                session.status,
                crate::domain::sessions::Status::Ready
                    | crate::domain::sessions::Status::Active
                    | crate::domain::sessions::Status::Paused
            ) {
                return Ok(None);
            }
            drop(conn);
            crate::enforcement::activate(&app, &state, &session.id)?;
            let conn = state.db.0.lock().unwrap();
            Ok(crate::subjects::language::view(&conn, &session.id)?
                .map(|lesson| serde_json::json!({ "kind": "language", "lesson": lesson })))
        }
        crate::classroom::SubjectKind::Engineering => {
            let conn = state.db.0.lock().unwrap();
            if let Some(lesson) =
                crate::classroom::active_engineering_session(&conn, spec.id).map_err(err)?
            {
                return Ok(Some(
                    serde_json::json!({ "kind": "engineering", "lesson": lesson }),
                ));
            }
            let Some(session) = crate::subjects::engineering::resumable(&conn, spec.id)? else {
                return Ok(None);
            };
            if !matches!(
                session.status,
                crate::domain::sessions::Status::Ready
                    | crate::domain::sessions::Status::Active
                    | crate::domain::sessions::Status::Paused
            ) {
                // Planned or failed preparation: Learn now retries it, Discard skips it.
                return Ok(None);
            }
            drop(conn);
            crate::enforcement::activate(&app, &state, &session.id)?;
            let conn = state.db.0.lock().unwrap();
            Ok(crate::subjects::engineering::view(&conn, &session.id)?
                .map(|lesson| serde_json::json!({ "kind": "engineering", "lesson": lesson })))
        }
    }
}

#[tauri::command]
pub fn submit_classroom_engineering_session(
    app: AppHandle,
    state: State<'_, AppState>,
    input: crate::classroom::SubmitEngineeringInput,
) -> CmdResult<crate::classroom::EngineeringSessionResult> {
    let result = {
        let conn = state.db.0.lock().unwrap();
        crate::classroom::submit_engineering_session(&conn, &input, &state.today()).map_err(err)?
    };
    let _ = app.emit("classroom:state", &result);
    Ok(result)
}

#[tauri::command]
pub fn submit_language_session(
    app: AppHandle,
    state: State<'_, AppState>,
    input: crate::language::SubmitSessionInput,
) -> CmdResult<crate::language::LanguageSessionResult> {
    let result = {
        let conn = state.db.0.lock().unwrap();
        crate::language::submit_session(&conn, &input, &state.today()).map_err(err)?
    };
    let _ = app.emit("classroom:state", &result);
    Ok(result)
}

#[derive(Serialize, Deserialize, Clone)]
pub struct QuizQuestionView {
    pub id: i64,
    pub prompt: String,
    pub kind: String,
    pub choices: Option<Vec<String>>,
    pub origin: String,
    pub answered: bool,
    pub draft: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct QuizRoundView {
    pub round_id: Option<RoundId>,
    pub revision: u32,
    pub questions: Vec<QuizQuestionView>,
}

#[tauri::command]
pub fn escape_session(
    app: AppHandle,
    state: State<'_, AppState>,
    phrase: String,
) -> CmdResult<bool> {
    match crate::kiosk::verify_escape(&state, &phrase)? {
        true => {
            // A focused class session is paused with its work intact. The retired
            // daily routine has no running day left to skip.
            let _ = crate::enforcement::escape(&app, &state);
            state.clear_chat_threads();
            crate::kiosk::release(&app, &state);
            let _ = app.emit("classroom:state", serde_json::json!({ "refresh": true }));
            let _ = app.emit("classroom:state", serde_json::json!({ "escaped": true }));
            Ok(true)
        }
        false => Ok(false),
    }
}

/// One line typed at the recovery console.
#[tauri::command]
pub fn recovery_command(
    app: AppHandle,
    state: State<'_, AppState>,
    recovery: State<'_, crate::recovery::RecoveryState>,
    line: String,
) -> CmdResult<crate::recovery::Reply> {
    if line.len() > 400 {
        return Err("that line is too long".into());
    }
    Ok(crate::recovery::command(&app, &state, &recovery, &line))
}

/// The combination as this platform names it, whether the system took it,
/// and the ladder: what Settings, onboarding and the console show.
#[tauri::command]
pub fn recovery_status(
    recovery: State<'_, crate::recovery::RecoveryState>,
) -> CmdResult<crate::recovery::Status> {
    Ok(crate::recovery::status(&recovery))
}

/// Open the console from Settings, to see it before it is needed.
#[tauri::command]
pub fn open_recovery_console(app: AppHandle) -> CmdResult<()> {
    crate::recovery::open_console(&app)
}

#[tauri::command]
pub fn close_recovery_console(app: AppHandle) -> CmdResult<()> {
    use tauri::Manager;
    if let Some(window) = app.get_webview_window(crate::recovery::WINDOW) {
        window.hide().map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_escape_phrase(state: State<'_, AppState>) -> CmdResult<String> {
    let conn = state.db.0.lock().unwrap();
    Ok(db::get_config(&conn, "escape_phrase")
        .map_err(err)?
        .unwrap_or_default())
}

#[tauri::command]
pub fn get_dashboard(
    state: State<'_, AppState>,
    query: Option<crate::progress::ProgressQuery>,
) -> CmdResult<crate::progress::DashboardView> {
    let conn = state.db.0.lock().unwrap();
    crate::progress::read(&conn, &state.today(), &query.unwrap_or_default())
}

/// The home page's habit view: streaks, half a year of days, this week.
#[tauri::command]
pub fn get_study_pulse(state: State<'_, AppState>) -> CmdResult<crate::progress::StudyPulse> {
    let conn = state.db.0.lock().unwrap();
    crate::progress::pulse(&conn, &state.today())
}

#[tauri::command]
pub fn get_progress_lesson(
    state: State<'_, AppState>,
    source: String,
    owner_id: String,
) -> CmdResult<Option<crate::progress::ProgressLesson>> {
    let conn = state.db.0.lock().unwrap();
    crate::progress::lesson(&conn, &source, &owner_id)
}

#[derive(Serialize)]
pub struct ArchivedCourse {
    pub course_id: i64,
    pub session_date: String,
    pub title: String,
    pub markdown: String,
    pub resources: serde_json::Value,
}

/// One exercise workspace view for either owner: a primary course or a
/// classroom session (exactly one of the two ids must be present).
#[tauri::command]
pub fn get_exercise(
    state: State<'_, AppState>,
    course_id: Option<i64>,
    classroom_session_id: Option<i64>,
    study_session_id: Option<String>,
) -> CmdResult<Option<db::ExerciseView>> {
    let conn = state.db.0.lock().unwrap();
    if let Some(id) = study_session_id {
        return crate::subjects::engineering::exercise_view(
            &conn,
            &crate::domain::sessions::SessionId(id),
        );
    }
    if let Some(id) = classroom_session_id {
        return crate::classroom::classroom_exercise(&conn, id).map_err(err);
    }
    let Some(id) = course_id else {
        return Err("exercise owner must be a course or a classroom session".into());
    };
    let Some(exercise) = db::get_course_exercise(&conn, id).map_err(err)? else {
        return Ok(None);
    };
    let draft = db::get_exercise_draft(&conn, Some(id), None).map_err(err)?;
    let (completed, reflection) =
        db::get_exercise_completion(&conn, Some(id), None).map_err(err)?;
    Ok(Some(db::ExerciseView {
        course_id: Some(exercise.course_id),
        classroom_session_id: None,
        study_session_id: None,
        title: exercise.title,
        instructions: exercise.instructions,
        starter_code: exercise.starter_code,
        deliverable: exercise.deliverable,
        hints: exercise.hints,
        draft,
        completed,
        reflection,
    }))
}

/// Debounced on the frontend; the backend just persists whatever draft
/// text it is given. Never touches the session timer, kiosk lock, or
/// mastery/completion state.
#[tauri::command]
pub fn save_exercise_draft(
    state: State<'_, AppState>,
    course_id: Option<i64>,
    classroom_session_id: Option<i64>,
    study_session_id: Option<String>,
    draft: String,
) -> CmdResult<()> {
    let conn = state.db.0.lock().unwrap();
    if let Some(id) = study_session_id {
        return crate::subjects::engineering::save_exercise_work(
            &conn,
            &crate::domain::sessions::SessionId(id),
            Some(draft),
            None,
        );
    }
    db::save_exercise_draft(&conn, course_id, classroom_session_id, &draft).map_err(err)
}

/// Self-certified practice evidence. Completion stays non-blocking, but the
/// reflection enters the learner dossier so future lessons can build on work
/// the student actually attempted rather than assuming every exercise was done.
#[tauri::command]
pub fn save_exercise_completion(
    state: State<'_, AppState>,
    course_id: Option<i64>,
    classroom_session_id: Option<i64>,
    study_session_id: Option<String>,
    completed: bool,
    reflection: String,
) -> CmdResult<()> {
    if completed && reflection.split_whitespace().count() < 5 {
        return Err(
            "add a short evidence/trade-off reflection before marking this complete".into(),
        );
    }
    let conn = state.db.0.lock().unwrap();
    if let Some(id) = study_session_id {
        return crate::subjects::engineering::save_exercise_work(
            &conn,
            &crate::domain::sessions::SessionId(id),
            None,
            Some((completed, reflection)),
        );
    }
    db::save_exercise_completion(
        &conn,
        course_id,
        classroom_session_id,
        completed,
        &reflection,
    )
    .map_err(err)
}

#[derive(Serialize, Clone)]
pub struct ChatMessageView {
    pub role: String,
    pub content: String,
    pub section: Option<String>,
    pub follow_ups: Vec<String>,
}

fn chat_view(turns: &[crate::generator::ChatTurn]) -> Vec<ChatMessageView> {
    turns
        .iter()
        .map(|t| ChatMessageView {
            role: t.role.clone(),
            content: t.content.clone(),
            section: t.section.clone(),
            follow_ups: t.follow_ups.clone(),
        })
        .collect()
}

const MAX_CHAT_MESSAGE_CHARS: usize = 2_000;

fn prepare_chat_message(message: String) -> CmdResult<String> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("message cannot be empty".into());
    }
    if message.chars().count() > MAX_CHAT_MESSAGE_CHARS {
        return Err(format!(
            "message is too long; keep it under {MAX_CHAT_MESSAGE_CHARS} characters"
        ));
    }
    Ok(message)
}

/// Current in-memory chat thread for one owner: a primary course or a
/// classroom session (exactly one id). Empty (never an error) when nothing
/// has been asked yet — the whole thread lives only for the active app
/// session and is gone on restart, completion, or skip.
#[tauri::command]
pub fn get_chat(
    state: State<'_, AppState>,
    course_id: Option<i64>,
    classroom_session_id: Option<i64>,
    study_session_id: Option<String>,
) -> CmdResult<Vec<ChatMessageView>> {
    let key = chat_key(course_id, classroom_session_id, study_session_id.as_deref())?;
    let threads = state.chat_threads.lock().unwrap();
    Ok(threads
        .get(&key)
        .map(|turns| chat_view(turns))
        .unwrap_or_default())
}

fn chat_key(
    course_id: Option<i64>,
    classroom_session_id: Option<i64>,
    study_session_id: Option<&str>,
) -> CmdResult<String> {
    match (course_id, classroom_session_id, study_session_id) {
        (Some(id), None, None) => Ok(format!("course:{id}")),
        (None, Some(id), None) => Ok(format!("classroom:{id}")),
        (None, None, Some(id)) if !id.is_empty() => Ok(format!("study:{id}")),
        _ => Err(
            "chat owner must be exactly one of course, classroom session or study session".into(),
        ),
    }
}

/// Ask one bounded, course-grounded question. Never touches the reading
/// timer, kiosk lock, mastery, or completion state — purely a session-only
/// side conversation about the course already on screen.
#[tauri::command]
pub async fn send_chat_message(
    state: State<'_, AppState>,
    course_id: Option<i64>,
    classroom_session_id: Option<i64>,
    study_session_id: Option<String>,
    message: String,
) -> CmdResult<Vec<ChatMessageView>> {
    let key = chat_key(course_id, classroom_session_id, study_session_id.as_deref())?;
    let message = prepare_chat_message(message)?;
    let history = state
        .chat_threads
        .lock()
        .unwrap()
        .get(&key)
        .cloned()
        .unwrap_or_default();
    let reply = if let Some(id) = study_session_id {
        let (context, profile) = {
            let conn = state.db.0.lock().unwrap();
            let id = crate::domain::sessions::SessionId(id);
            (
                crate::subjects::engineering::chat_context(&conn, &id)?,
                crate::subjects::engineering::generation_profile(&conn, &id)?,
            )
        };
        state
            .generator
            .answer_course_question_for(&context, &message, &history, &profile)
            .await
            .map_err(err)?
    } else if let Some(id) = classroom_session_id {
        let (context, profile) = {
            let conn = state.db.0.lock().unwrap();
            let context = crate::classroom::engineering_chat_context(&conn, id).map_err(err)?;
            let program = crate::classroom::program_row(&conn, &context.focus).map_err(err)?;
            let profile = crate::classroom::generation_profile(&program);
            (context, profile)
        };
        state
            .generator
            .answer_course_question_for(&context, &message, &history, &profile)
            .await
            .map_err(err)?
    } else {
        let course_id = course_id.ok_or("chat owner must be a course")?;
        let context = {
            let conn = state.db.0.lock().unwrap();
            let course = db::get_course(&conn, course_id)
                .map_err(err)?
                .ok_or("course not found")?;
            let concept = db::get_concept(&conn, course.concept_id)
                .map_err(err)?
                .ok_or("course concept not found")?;
            let exercise = db::get_course_exercise(&conn, course_id)
                .map_err(err)?
                .map(|value| serde_json::to_string_pretty(&value))
                .transpose()
                .map_err(err)?
                .unwrap_or_else(|| "(this course has no separate exercise)".into());
            crate::generator::CourseChatContext {
                title: concept.title,
                focus: concept.focus,
                markdown: course.markdown,
                learner_outcome: concept.curriculum.learner_outcome,
                cumulative_artifact: concept.curriculum.artifact,
                exercise,
            }
        };
        state
            .generator
            .answer_course_question(&context, &message, &history)
            .await
            .map_err(err)?
    };
    let mut threads = state.chat_threads.lock().unwrap();
    let thread = threads.entry(key).or_default();
    thread.push(crate::generator::ChatTurn::user(message));
    thread.push(crate::generator::ChatTurn::assistant(reply));
    Ok(chat_view(thread))
}
