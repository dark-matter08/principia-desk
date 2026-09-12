pub mod agents;
pub mod alarm;
pub mod audio;
pub mod catalog;
pub mod class_builder;
pub mod classroom;
pub mod commands;
pub mod db;
pub mod domain;
pub mod enforcement;
pub mod execution_log;
pub mod focus;
pub mod generator;
pub mod keychain;
pub mod kiosk;
pub mod language;
pub mod lesson_export;
pub mod lesson_shape;
pub mod mastery;
#[cfg(target_os = "macos")]
pub mod menu_panel;
pub mod progress;
pub mod prose;
pub mod readiness;
pub mod recovery;
pub mod research;
pub mod scheduler;
pub mod search;
pub mod selection;
pub mod state;
pub mod storage;
pub mod subjects;
pub mod tray;

use state::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

const SEED_CONCEPTS: &str = include_str!("../seed/concepts.json");

fn resolve_claude_bin() -> String {
    if let Ok(p) = std::env::var("PRINCIPIA_CLAUDE_BIN") {
        return p;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{home}/.local/bin/claude"),
        "/usr/local/bin/claude".to_string(),
        "/opt/homebrew/bin/claude".to_string(),
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return c.clone();
        }
    }
    "claude".to_string()
}

fn resolve_codex_bin(conn: &rusqlite::Connection) -> Option<String> {
    match std::env::var("PRINCIPIA_CODEX_BIN").as_deref() {
        Ok("none") => return Some("none".into()),
        Ok(p) => return Some(p.to_string()),
        _ => {}
    }
    if let Ok(Some(saved)) = db::get_config(conn, "codex_bin") {
        if std::path::Path::new(&saved).exists() {
            return Some(saved);
        }
    }
    let path = agents::process::resolve("codex")?;
    let _ = db::set_config(conn, "codex_bin", &path);
    Some(path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args: Vec<String> = std::env::args().collect();
    let triggered = args.iter().any(|a| a == "--triggered");
    let debug_day = args.iter().any(|a| a == "--debug-day");
    let context = tauri::generate_context!();
    log::info!(
        "starting {} with {} configured window(s); debug study mode: {}",
        context.config().identifier,
        context.config().app.windows.len(),
        debug_day
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch (e.g. launchd firing while we run) just surfaces the window.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
            let state = app.state::<AppState>();
            let classroom_slots = {
                let conn = state.db.0.lock().unwrap();
                let _ = classroom::refresh_appointments(&conn, &state.today());
                classroom::slot_views(&conn, &state.today(), state.debug_day)
            };
            if let Ok(slots) = classroom_slots {
                let due = slots
                    .into_iter()
                    .filter(|slot| slot.owed)
                    .collect::<Vec<_>>();
                if !due.is_empty() {
                    let _ = app.emit("classroom:owed", due);
                }
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            log::info!("initializing local study storage");
            let data_dir = app.path().app_data_dir().expect("app data dir resolvable");
            std::fs::create_dir_all(&data_dir)?;
            let database = data_dir.join("principia.db");
            match storage::adoption::adopt(&data_dir, &database) {
                Ok(Some(source)) => log::info!("adopted the profile from {}", source.display()),
                Ok(None) => {}
                Err(error) => log::error!("could not adopt the earlier profile: {error}"),
            }
            scheduler::retire_legacy();
            let conn = db::open(&database)?;
            match execution_log::prune(&conn, chrono::Utc::now()) {
                Ok(0) => {}
                Ok(count) => log::info!("pruned {count} runner log line(s) past retention"),
                Err(error) => log::warn!("could not prune the runner log: {error}"),
            }
            match domain::sessions::release_orphaned_preparations(&conn, chrono::Utc::now()) {
                Ok(0) => {}
                Ok(count) => log::warn!("released {count} preparation lease(s) orphaned by the last run"),
                Err(error) => log::error!("could not release orphaned preparations: {error}"),
            }
            db::seed_concepts(&conn, SEED_CONCEPTS)?;
            let startup_today = chrono::Local::now().format("%Y-%m-%d").to_string();
            language::initialize(&conn, &startup_today).map_err(std::io::Error::other)?;
            classroom::initialize(&conn).map_err(std::io::Error::other)?;
            // One-time, idempotent import of finished daily-routine history into
            // the shared runtime. Open work stays with the compatibility engine.
            match storage::primary_import::apply(&conn) {
                Ok(summary) => log::info!(
                    "legacy study import: {} imported, {} already imported, {} retained for recovery, {} open",
                    summary.imported.len(),
                    summary.already_imported,
                    summary.retained.len(),
                    summary.open_work
                ),
                Err(error) => log::error!("legacy study import skipped: {error}"),
            }
            let codex_bin = resolve_codex_bin(&conn);
            // Primary model for course generation: config 'model' (default opus).
            // Held behind Arc<Mutex> so settings changes apply live.
            let model = std::sync::Arc::new(Mutex::new(
                db::get_config(&conn, "model")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "opus".to_string()),
            ));
            let agent = std::sync::Arc::new(Mutex::new(
                db::get_config(&conn, "agent")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "claude".to_string()),
            ));
            let custom_bin = std::sync::Arc::new(Mutex::new(
                db::get_config(&conn, "custom_agent_bin")
                    .ok()
                    .flatten()
                    .unwrap_or_default(),
            ));
            // Live agent-activity feed: generator -> broadcast -> gen:log events.
            let (log_tx, mut log_rx) =
                tokio::sync::broadcast::channel::<execution_log::Reported>(256);
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        let reported = match log_rx.recv().await {
                            Ok(reported) => reported,
                            // Falling behind loses lines; it must not end the feed.
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                                log::warn!("the execution log fell behind by {missed} line(s)");
                                continue;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        };
                        let _ = handle.emit("gen:log", &reported.line);
                        let state = handle.state::<AppState>();
                        let conn = state.db.0.lock().unwrap();
                        let run = reported.run.as_ref().map(|(r, c)| (r.as_str(), c.as_str()));
                        if let Err(error) =
                            execution_log::append(&conn, run, &reported.line, chrono::Utc::now())
                        {
                            log::warn!("could not store a runner line: {error}");
                        }
                    }
                });
            }
            let mut generator = generator::Generator::new(
                resolve_claude_bin(),
                codex_bin,
                data_dir.join("scratch"),
                model,
                agent,
                custom_bin,
                execution_log::Feed::new(log_tx),
            );
            generator.runner.database = Some(data_dir.join("principia.db"));
            generator.researcher.set_search(search::load(&conn));
            app.manage(recovery::RecoveryState::default());
            app.manage(class_builder::BuilderJobs::default());
            recovery::install(app.handle());
            app.manage(AppState {
                db: db::Db(Mutex::new(conn)),
                generator,
                data_dir,
                locked: AtomicBool::new(false),
                alarm_ringing: AtomicBool::new(false),
                alarm_for: Mutex::new(None),
                panel_height: Mutex::new(600.0),
                preparing_ahead: AtomicBool::new(false),
                debug_day,
                escape_failures: Mutex::new(Vec::new()),
                prev_muted: Mutex::new(None),
                frontend_ready: AtomicBool::new(false),
                class_start_gate: tokio::sync::Mutex::new(()),
                chat_threads: Mutex::new(std::collections::HashMap::new()),
                focus: enforcement::Coordinator::default(),
            });

            // Self-heal the launchd plist if it points at a stale binary path
            // (e.g. setup completed from a dev build).
            if !debug_day {
                let state = app.state::<AppState>();
                let conn = state.db.0.lock().unwrap();
                let onboarded =
                    matches!(db::get_config(&conn, "onboarded"), Ok(Some(v)) if v == "1");
                let paused =
                    matches!(db::get_config(&conn, "schedule_paused"), Ok(Some(v)) if v == "1");
                if onboarded {
                    let times = if paused {
                        Ok(Vec::new())
                    } else {
                        classroom::all_schedule_times(&conn)
                    };
                    drop(conn);
                    match times {
                        Ok(times) => scheduler::ensure_current_many(&times),
                        Err(error) => log::error!("could not reconcile class wakeups: {error}"),
                    }
                }
            }

            // Owed-session watcher: checks every 60s (covers app-already-running case).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    let state = handle.state::<AppState>();
                    let classroom_slots = {
                        let conn = state.db.0.lock().unwrap();
                        let _ = classroom::refresh_appointments(&conn, &state.today());
                        classroom::slot_views(&conn, &state.today(), state.debug_day)
                    };
                    if let Ok(slots) = classroom_slots {
                        let due = slots
                            .into_iter()
                            .filter(|slot| slot.owed)
                            .collect::<Vec<_>>();
                        if !due.is_empty() {
                            // Classroom reminders are deliberately advisory:
                            // surface them in-app without taking the kiosk lock.
                            let _ = handle.emit("classroom:owed", due);
                        }
                    }
                    readiness::tick(&handle);
                    alarm::evaluate(&handle);
                }
            });

            // Launched by launchd at the scheduled time (or at load for catch-up).
            if triggered {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    let state = handle.state::<AppState>();
                    let classroom_slots = {
                        let conn = state.db.0.lock().unwrap();
                        let _ = classroom::refresh_appointments(&conn, &state.today());
                        classroom::slot_views(&conn, &state.today(), state.debug_day)
                    };
                    if let Ok(slots) = classroom_slots {
                        let due = slots
                            .into_iter()
                            .filter(|slot| slot.owed)
                            .collect::<Vec<_>>();
                        if !due.is_empty() {
                            let _ = handle.emit("classroom:owed", due);
                        }
                    }
                    readiness::tick(&handle);
                    alarm::evaluate(&handle);
                });
            }
            if let Err(error) = tray::install(app.handle()) {
                log::error!("menu bar icon unavailable: {error}");
            }
            alarm::evaluate(app.handle());
            log::info!("local study runtime ready");
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // The desk stays resident: closing the window hides it behind
                // the menu bar icon, and a lock keeps it in front.
                api.prevent_close();
                if window.label() == tray::PANEL_LABEL {
                    let _ = window.hide();
                    return;
                }
                let state = window.app_handle().state::<AppState>();
                if state.debug_day || !state.locked.load(Ordering::SeqCst) {
                    let _ = window.hide();
                }
            }
            tauri::WindowEvent::Focused(false) => {
                if window.label() == tray::PANEL_LABEL {
                    tray::hide_panel(window.app_handle());
                    return;
                }
                let state = window.app_handle().state::<AppState>();
                if !state.debug_day && state.locked.load(Ordering::SeqCst) {
                    let _ = window.set_focus();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::mark_frontend_ready,
            commands::report_frontend_error,
            commands::get_app_state,
            commands::get_catalog,
            commands::get_enrollment_options,
            commands::get_enrollment_draft,
            commands::save_enrollment_draft,
            commands::placement::get_placement_check,
            commands::placement::start_placement_check,
            commands::placement::save_placement_response,
            commands::placement::submit_placement_round,
            commands::placement::continue_placement_check,
            commands::placement::finish_placement_check,
            commands::placement::get_path_recommendation,
            commands::check_agent,
            commands::agents::list_agent_runners,
            commands::agents::get_runner_models,
            commands::classes::get_class_path,
            commands::classes::accept_class_path,
            commands::classes::revise_class_path,
            commands::custom::list_custom_courses,
            commands::custom::get_custom_course,
            commands::custom::create_custom_course,
            commands::custom::save_custom_course_brief,
            commands::custom::save_custom_course_draft,
            commands::custom::draft_custom_course,
            commands::custom::review_custom_course,
            commands::custom::verify_custom_course_sources,
            commands::custom::write_custom_course_bank,
            commands::custom::void_custom_question,
            commands::custom::get_practice_bank,
            commands::custom::write_practice_questions,
            commands::custom::void_practice_question,
            commands::custom::clear_practice_bank,
            commands::custom::fix_custom_course_finding,
            commands::custom::fix_all_custom_course_findings,
            commands::custom::resolve_custom_course_finding,
            commands::custom::accept_custom_course_source,
            commands::custom::replace_custom_course_source,
            commands::custom::replace_all_custom_course_sources,
            commands::custom::mark_custom_course_read,
            commands::custom::publish_custom_course,
            commands::custom::delete_custom_course_draft,
            commands::custom::export_custom_course,
            commands::custom::import_custom_course,
            commands::challenges::get_unit_challenge,
            commands::challenges::start_unit_challenge,
            commands::challenges::save_unit_challenge_response,
            commands::challenges::submit_unit_challenge_round,
            commands::challenges::apply_unit_challenge,
            commands::agents::get_runner_configuration,
            commands::agents::save_runner_configuration,
            commands::agents::set_runner_key,
            commands::agents::get_local_models,
            commands::agents::get_local_pulls,
            commands::agents::install_local_runner,
            commands::agents::start_local_runner,
            commands::agents::pull_local_model,
            commands::agents::remove_local_model,
            commands::agents::select_runner,
            commands::agents::set_openrouter_free_only,
            commands::agents::get_openrouter_free_only,
            commands::agents::test_agent_connection,
            commands::agents::get_agent_activity,
            commands::agents::get_agent_policy,
            commands::agents::set_agent_policy,
            commands::complete_setup,
            commands::portability::export_profile,
            commands::portability::inspect_archive,
            commands::portability::import_profile,
            commands::portability::reveal_export,
            commands::portability::export_lesson_csv,
            commands::portability::get_lesson_document,
            commands::portability::save_lesson_pdf,
            commands::get_study_pulse,
            commands::search::get_search_settings,
            commands::search::set_search_settings,
            commands::search::set_search_key,
            commands::search::test_search,
            commands::get_curriculum_map,
            commands::configure_classroom_program,
            commands::upsert_classroom_slot,
            commands::delete_classroom_slot,
            commands::plan_classroom_schedule,
            commands::start_classroom_session,
            commands::resume_classroom_session,
            commands::start_class_review,
            commands::submit_classroom_engineering_session,
            commands::save_class_lesson_work,
            commands::save_class_check_answer,
            commands::submit_class_check,
            commands::submit_class_language_check,
            commands::pause_class_lesson,
            commands::skip_class_lesson,
            commands::skip_appointment,
            commands::snooze_alarm,
            commands::end_block,
            commands::list_execution_runs,
            commands::get_execution_log,
            commands::get_recent_execution_log,
            commands::show_desk,
            commands::start_from_tray,
            commands::quit_desk,
            commands::hide_tray_panel,
            commands::size_tray_panel,
            commands::reschedule_appointment,
            commands::set_class_focus_policy,
            commands::get_class_appointments,
            commands::submit_language_session,
            commands::set_kiosk_level,
            commands::set_model,
            commands::set_agent,
            commands::set_deepseek_api_key,
            commands::pause_schedule,
            commands::resume_schedule,
            commands::escape_session,
            commands::recovery_command,
            commands::recovery_status,
            commands::open_recovery_console,
            commands::close_recovery_console,
            commands::get_escape_phrase,
            commands::get_dashboard,
            commands::get_progress_lesson,
            commands::get_exercise,
            commands::save_exercise_draft,
            commands::save_exercise_completion,
            commands::get_chat,
            commands::send_chat_message,
        ])
        .build(context)
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Ready = event {
                for (label, window) in app.webview_windows() {
                    log::info!(
                        "desktop window {label}: visible={:?}, size={:?}",
                        window.is_visible(),
                        window.inner_size()
                    );
                }
            }
            // Clicking the Dock icon while the window is hidden brings the desk back.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } = event
            {
                tray::show_window(app);
            }
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                let state = app.state::<AppState>();
                // Cmd+Q and the last window closing arrive without a code: the
                // runner stays up and the window hides. The menu bar's Quit
                // exits with a code, and it already refuses while an alarm
                // rings or a session holds the desk.
                if code.is_none() {
                    api.prevent_exit();
                    if let Some(window) = app.get_webview_window("main") {
                        if state.debug_day || !state.locked.load(Ordering::SeqCst) {
                            let _ = window.hide();
                        }
                    }
                } else if !state.debug_day && state.locked.load(Ordering::SeqCst) {
                    api.prevent_exit();
                }
            }
        });
}
