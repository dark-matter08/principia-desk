use super::{err, CmdResult};
use crate::{
    domain::classes::{self, AcceptPath, AcceptedPath, RevisePath},
    state::AppState,
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn get_class_path(
    state: State<'_, AppState>,
    course_id: String,
) -> CmdResult<Option<AcceptedPath>> {
    classes::current_path(&state.db.0.lock().unwrap(), &course_id).map_err(err)
}

#[tauri::command]
pub fn accept_class_path(
    app: AppHandle,
    state: State<'_, AppState>,
    input: AcceptPath,
) -> CmdResult<AcceptedPath> {
    let result =
        classes::accept(&state.db.0.lock().unwrap(), &input, &state.today()).map_err(err)?;
    // Retrying after an OS-registration failure reuses the accepted revision.
    super::refresh_os_schedule(&state)?;
    let _ = app.emit("classroom:state", result.summary());
    Ok(result)
}

/// Bypass or include topics on an accepted engineering route. Future
/// selections follow the new revision; nothing is graded or credited.
#[tauri::command]
pub fn revise_class_path(
    app: AppHandle,
    state: State<'_, AppState>,
    input: RevisePath,
) -> CmdResult<AcceptedPath> {
    let result =
        classes::revise(&state.db.0.lock().unwrap(), &input, &state.today()).map_err(err)?;
    let _ = app.emit("classroom:state", result.summary());
    Ok(result)
}
