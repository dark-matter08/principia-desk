//! Commands for listening mode: the engines and voices on this machine, a
//! class's choice, and the audio of one lesson.

use crate::audio;
use crate::domain::enrollment::AudioPreference;
use crate::state::AppState;
use tauri::{AppHandle, Manager, State};

type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub async fn get_audio_status(state: State<'_, AppState>) -> CmdResult<audio::AudioStatus> {
    Ok(audio::status(&state.data_dir).await)
}

/// Build an engine's environment, every step on the Logs page.
#[tauri::command]
pub async fn install_audio_engine(
    state: State<'_, AppState>,
    engine: String,
) -> CmdResult<Vec<crate::pyenv::Step>> {
    let _run = state
        .generator
        .feed
        .begin(&format!("voices:{engine}"), "desk");
    audio::install_engine(&state.generator.feed, &state.data_dir, &engine).await
}

#[tauri::command]
pub async fn remove_audio_engine(state: State<'_, AppState>, engine: String) -> CmdResult<()> {
    audio::remove_engine(&state.data_dir, &engine)
}

#[tauri::command]
pub async fn install_voice(
    state: State<'_, AppState>,
    voice: String,
) -> CmdResult<Vec<crate::pyenv::Step>> {
    let _run = state
        .generator
        .feed
        .begin(&format!("voices:{voice}"), "desk");
    audio::install_voice(&state.generator.feed, &state.data_dir, &voice).await
}

#[tauri::command]
pub async fn remove_voice(state: State<'_, AppState>, voice: String) -> CmdResult<()> {
    audio::remove_voice(&state.data_dir, &voice)
}

/// A sample sentence in one voice; the path of the rendered file.
#[tauri::command]
pub async fn preview_voice(
    state: State<'_, AppState>,
    engine: String,
    voice: String,
) -> CmdResult<String> {
    let _run = state
        .generator
        .feed
        .begin(&format!("voices:{voice}"), "desk");
    let path = audio::preview(&state.generator.feed, &state.data_dir, &engine, &voice).await?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn get_class_audio(
    state: State<'_, AppState>,
    course_id: String,
) -> CmdResult<AudioPreference> {
    let conn = state.db.0.lock().unwrap();
    Ok(audio::preference_for(&conn, &course_id))
}

/// Choose whether and how a class's lessons are voiced; the next lesson
/// prepared reads it.
#[tauri::command]
pub fn set_class_audio(
    state: State<'_, AppState>,
    course_id: String,
    preference: AudioPreference,
) -> CmdResult<AudioPreference> {
    if !preference.engine.is_empty() && !audio::ENGINES.contains(&preference.engine.as_str()) {
        return Err(format!("unknown engine {}", preference.engine));
    }
    let conn = state.db.0.lock().unwrap();
    let config =
        crate::domain::classes::set_audio_preference(&conn, &course_id, preference, &state.today())
            .map_err(|e| e.to_string())?;
    Ok(config.audio)
}

#[tauri::command]
pub fn get_lesson_audio(
    state: State<'_, AppState>,
    session_id: String,
) -> CmdResult<Option<audio::LessonAudio>> {
    let conn = state.db.0.lock().unwrap();
    audio::lesson_audio(&conn, &session_id).map_err(|e| e.to_string())
}

/// Write and voice the audio for one lesson now, whatever the class chose.
/// Returns at once; `audio:state` events say how it goes.
#[tauri::command]
pub fn write_lesson_audio(app: AppHandle, session_id: String) -> CmdResult<()> {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = audio::prepare_for_session(&handle, &session_id, true).await {
            log::warn!("audio for {session_id}: {error}");
        }
    });
    let _ = app.state::<AppState>();
    Ok(())
}
