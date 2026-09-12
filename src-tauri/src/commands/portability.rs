//! Commands for taking a profile off this machine and putting one back.

use crate::domain::portability::{self, ArchiveSummary};
use crate::state::AppState;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};

type CmdResult<T> = Result<T, String>;
fn err<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub path: String,
    pub summary: ArchiveSummary,
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub rows: usize,
    /// Where the record that was replaced is kept, so it can be restored.
    pub backup_path: String,
    pub summary: ArchiveSummary,
}

/// Where an export goes: the learner's documents if that exists, otherwise
/// beside the database, which always does.
fn export_directory(app: &AppHandle, state: &AppState) -> std::path::PathBuf {
    let documents = app
        .path()
        .document_dir()
        .ok()
        .map(|dir| dir.join("Principia Desk"));
    match documents {
        Some(dir) if std::fs::create_dir_all(&dir).is_ok() => dir,
        _ => state.data_dir.join("exports"),
    }
}

#[tauri::command]
pub fn export_profile(app: AppHandle, state: State<'_, AppState>) -> CmdResult<ExportResult> {
    let archive = {
        let conn = state.db.0.lock().unwrap();
        portability::export(&conn, env!("CARGO_PKG_VERSION")).map_err(err)?
    };
    let summary = portability::summarize(&archive);
    let directory = export_directory(&app, &state);
    std::fs::create_dir_all(&directory).map_err(err)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = directory.join(format!("principia-desk-{stamp}.json"));
    let document = serde_json::to_string(&archive).map_err(err)?;
    std::fs::write(&path, document).map_err(err)?;
    Ok(ExportResult {
        path: path.to_string_lossy().into_owned(),
        summary,
    })
}

/// Read an archive and report what it holds. Nothing is written, so a learner
/// can see what a file would replace before agreeing to it.
#[tauri::command]
pub fn inspect_archive(document: String) -> CmdResult<ArchiveSummary> {
    portability::read(&document)
        .map(|archive| portability::summarize(&archive))
        .map_err(err)
}

#[tauri::command]
pub fn import_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    document: String,
) -> CmdResult<ImportResult> {
    // A focused session owns the desk and its lesson rows; replacing the
    // record underneath it would strand both.
    if crate::enforcement::view(&state).is_some() {
        return Err("Finish, pause or skip the active session before importing a profile.".into());
    }
    let archive = portability::read(&document).map_err(err)?;
    let summary = portability::summarize(&archive);
    let database = state.data_dir.join("principia.db");
    let backup = crate::storage::migrations::publish_backup(&database, "before-import")
        .map_err(|error| format!("the profile was not touched: {error}"))?;
    let rows = {
        let mut conn = state.db.0.lock().unwrap();
        portability::restore(&mut conn, &archive).map_err(err)?
    };
    state.clear_chat_threads();
    let _ = app.emit("classroom:state", serde_json::json!({ "refresh": true }));
    Ok(ImportResult {
        rows,
        backup_path: backup.to_string_lossy().into_owned(),
        summary,
    })
}

/// Reveal an exported file in the system file browser.
#[derive(Debug, Serialize)]
pub struct LessonFileResult {
    pub path: String,
    pub file_name: String,
    pub title: String,
    pub questions: usize,
    /// Whether the file carries correct answers and explanations. It does
    /// once the lesson's check has been submitted.
    pub answer_key: bool,
}

/// Lessons are filed by class: `Documents/Principia Desk/lessons/<class>/`.
fn lesson_directory(app: &AppHandle, state: &AppState, class_label: &str) -> PathBuf {
    let folder: String = class_label
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let folder = folder.trim().trim_matches('.').to_string();
    export_directory(app, state)
        .join("lessons")
        .join(if folder.is_empty() {
            "Other".to_string()
        } else {
            folder
        })
}

/// A file of the same name from an earlier download is left alone; the new
/// one gets a time suffix.
fn fresh_path(directory: &Path, stem: &str, extension: &str) -> PathBuf {
    let mut path = directory.join(format!("{stem}.{extension}"));
    if path.exists() {
        let stamp = chrono::Local::now().format("%H%M%S");
        path = directory.join(format!("{stem}-{stamp}.{extension}"));
    }
    path
}

fn file_result(path: &Path, document: &crate::lesson_export::LessonDocument) -> LessonFileResult {
    LessonFileResult {
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.to_string_lossy().into_owned(),
        title: document.title.clone(),
        questions: document.questions.len(),
        answer_key: document.answer_key,
    }
}

/// One saved lesson, gathered for the PDF the desk lays out in the webview.
#[tauri::command]
pub fn get_lesson_document(
    state: State<'_, AppState>,
    source: String,
    owner_id: String,
) -> CmdResult<crate::lesson_export::LessonDocument> {
    let conn = state.db.0.lock().unwrap();
    crate::lesson_export::document(&conn, &source, &owner_id)
}

/// Write one saved lesson, with its questions, as a CSV file in its class's
/// folder beside the profile exports.
#[tauri::command]
pub fn export_lesson_csv(
    app: AppHandle,
    state: State<'_, AppState>,
    source: String,
    owner_id: String,
) -> CmdResult<LessonFileResult> {
    let document = {
        let conn = state.db.0.lock().unwrap();
        crate::lesson_export::document(&conn, &source, &owner_id)?
    };
    let directory = lesson_directory(&app, &state, &document.class_label);
    std::fs::create_dir_all(&directory).map_err(err)?;
    let path = fresh_path(&directory, &document.file_stem, "csv");
    std::fs::write(&path, document.csv()).map_err(err)?;
    Ok(file_result(&path, &document))
}

/// Write the PDF the webview laid out for one saved lesson. The bytes come
/// as the raw request body; the lesson's source and owner ride in headers.
#[tauri::command]
pub fn save_lesson_pdf(
    app: AppHandle,
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> CmdResult<LessonFileResult> {
    let header = |name: &str| -> CmdResult<String> {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .ok_or_else(|| format!("missing {name} header"))
    };
    let source = header("x-lesson-source")?;
    let owner_id = header("x-lesson-owner")?;
    let bytes: Vec<u8> = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.clone(),
        tauri::ipc::InvokeBody::Json(_) => return Err("the PDF must be sent as bytes".into()),
    };
    if !bytes.starts_with(b"%PDF-") {
        return Err("that is not a PDF document".into());
    }
    let document = {
        let conn = state.db.0.lock().unwrap();
        crate::lesson_export::document(&conn, &source, &owner_id)?
    };
    let directory = lesson_directory(&app, &state, &document.class_label);
    std::fs::create_dir_all(&directory).map_err(err)?;
    let path = fresh_path(&directory, &document.file_stem, "pdf");
    std::fs::write(&path, bytes).map_err(err)?;
    Ok(file_result(&path, &document))
}

#[tauri::command]
pub fn reveal_export(app: AppHandle, path: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().reveal_item_in_dir(&path).map_err(err)
}
