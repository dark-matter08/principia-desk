//! Thin unit-challenge IPC. Lifecycle, grading and path effects live in domain.
use super::{err, CmdResult};
use crate::{
    domain::{
        assessments::{Response, RoundId},
        challenges::{self, UnitChallengeView},
        classes::AcceptedPath,
    },
    state::AppState,
};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn get_unit_challenge(
    state: State<'_, AppState>,
    course_id: String,
) -> CmdResult<Option<UnitChallengeView>> {
    challenges::get(&state.db.0.lock().unwrap(), &course_id).map_err(err)
}
#[tauri::command]
pub fn start_unit_challenge(
    state: State<'_, AppState>,
    course_id: String,
    unit: String,
    restart: bool,
) -> CmdResult<UnitChallengeView> {
    challenges::start(&state.db.0.lock().unwrap(), &course_id, &unit, restart).map_err(err)
}
#[tauri::command]
pub fn save_unit_challenge_response(
    state: State<'_, AppState>,
    course_id: String,
    round_id: RoundId,
    expected_revision: u32,
    question_id: String,
    response: Response,
) -> CmdResult<u32> {
    challenges::save_response(
        &state.db.0.lock().unwrap(),
        &course_id,
        &round_id,
        expected_revision,
        &question_id,
        response,
    )
    .map_err(err)
}
#[tauri::command]
pub fn submit_unit_challenge_round(
    state: State<'_, AppState>,
    course_id: String,
    round_id: RoundId,
    expected_revision: u32,
) -> CmdResult<UnitChallengeView> {
    challenges::submit(
        &state.db.0.lock().unwrap(),
        &course_id,
        &round_id,
        expected_revision,
    )
    .map_err(err)
}
#[tauri::command]
pub fn apply_unit_challenge(
    app: AppHandle,
    state: State<'_, AppState>,
    course_id: String,
    attempt_id: String,
    expected_revision: u32,
) -> CmdResult<AcceptedPath> {
    let result = challenges::apply(
        &state.db.0.lock().unwrap(),
        &course_id,
        &attempt_id,
        expected_revision,
        &state.today(),
    )
    .map_err(err)?;
    let _ = app.emit("classroom:state", result.summary());
    Ok(result)
}
