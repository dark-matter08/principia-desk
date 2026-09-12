//! Thin placement IPC. Lifecycle, frozen definitions and grading live in domain.
use super::{err, CmdResult};
use crate::{
    domain::{
        assessments::{Response, RoundId},
        enrollment::EnrollmentDraftId,
        placement::{self, DiagnosticView, Recommendation},
    },
    state::AppState,
};
use tauri::State;
#[tauri::command]
pub fn get_placement_check(
    state: State<'_, AppState>,
    draft_id: EnrollmentDraftId,
) -> CmdResult<Option<DiagnosticView>> {
    placement::get(&state.db.0.lock().unwrap(), &draft_id).map_err(err)
}
#[tauri::command]
pub fn start_placement_check(
    state: State<'_, AppState>,
    draft_id: EnrollmentDraftId,
    expected_revision: u32,
    restart: bool,
) -> CmdResult<DiagnosticView> {
    placement::start(
        &state.db.0.lock().unwrap(),
        &draft_id,
        expected_revision,
        restart,
    )
    .map_err(err)
}
#[tauri::command]
pub fn save_placement_response(
    state: State<'_, AppState>,
    draft_id: EnrollmentDraftId,
    round_id: RoundId,
    expected_revision: u32,
    question_id: String,
    response: Response,
) -> CmdResult<u32> {
    placement::save_response(
        &state.db.0.lock().unwrap(),
        &draft_id,
        &round_id,
        expected_revision,
        &question_id,
        response,
    )
    .map_err(err)
}
#[tauri::command]
pub fn submit_placement_round(
    state: State<'_, AppState>,
    draft_id: EnrollmentDraftId,
    round_id: RoundId,
    expected_revision: u32,
) -> CmdResult<DiagnosticView> {
    placement::submit(
        &state.db.0.lock().unwrap(),
        &draft_id,
        &round_id,
        expected_revision,
    )
    .map_err(err)
}
#[tauri::command]
pub fn continue_placement_check(
    state: State<'_, AppState>,
    draft_id: EnrollmentDraftId,
    round_id: RoundId,
) -> CmdResult<DiagnosticView> {
    placement::follow_up(&state.db.0.lock().unwrap(), &draft_id, &round_id).map_err(err)
}
#[tauri::command]
pub fn finish_placement_check(
    state: State<'_, AppState>,
    draft_id: EnrollmentDraftId,
    round_id: RoundId,
) -> CmdResult<DiagnosticView> {
    placement::finish(&state.db.0.lock().unwrap(), &draft_id, &round_id).map_err(err)
}
#[tauri::command]
pub fn get_path_recommendation(
    state: State<'_, AppState>,
    draft_id: EnrollmentDraftId,
    expected_revision: u32,
) -> CmdResult<Recommendation> {
    placement::recommend(&state.db.0.lock().unwrap(), &draft_id, expected_revision).map_err(err)
}
