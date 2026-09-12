//! Audio-lesson mode (TEACHER.md §5a): the Teacher writes a two-host dialogue
//! script; playback is tiered. Baseline engine is the webview's native
//! speechSynthesis (zero deps, offline). If the VibeVoice venv is provisioned
//! (scripts/provision-vibevoice.sh), lines are rendered to per-line WAV files
//! overnight and the player streams those instead.

use crate::db;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptLine {
    pub speaker: String, // "teacher" | "student"
    pub text: String,
    /// Absolute path to a rendered segment (vibevoice tier only).
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeneratedScript {
    pub lines: Vec<ScriptLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioView {
    pub lines: Vec<ScriptLine>,
    pub engine: String,
}

pub fn get_script(conn: &Connection, course_id: i64) -> db::Result<Option<AudioView>> {
    let mut stmt = conn
        .prepare("SELECT lines_json, engine, audio_dir FROM audio_scripts WHERE course_id = ?1")?;
    let mut rows = stmt.query(params![course_id])?;
    Ok(match rows.next()? {
        Some(r) => {
            let lines_json: String = r.get(0)?;
            let engine: String = r.get(1)?;
            let audio_dir: Option<String> = r.get(2)?;
            let mut lines: Vec<ScriptLine> = serde_json::from_str(&lines_json).unwrap_or_default();
            // Attach rendered files when they exist; degrade to speech if not.
            let mut have_files = false;
            if let Some(dir) = &audio_dir {
                for (i, l) in lines.iter_mut().enumerate() {
                    let f = Path::new(dir).join(format!("seg_{i:03}.wav"));
                    if f.exists() {
                        l.file = Some(f.to_string_lossy().to_string());
                        have_files = true;
                    }
                }
            }
            Some(AudioView {
                lines,
                engine: if have_files { engine } else { "speech".into() },
            })
        }
        None => None,
    })
}

pub fn save_script(conn: &Connection, course_id: i64, lines: &[ScriptLine]) -> db::Result<()> {
    conn.execute(
        "INSERT INTO audio_scripts (course_id, lines_json, engine, created_at)
         VALUES (?1, ?2, 'speech', datetime('now'))
         ON CONFLICT(course_id) DO UPDATE SET lines_json = excluded.lines_json",
        params![course_id, serde_json::to_string(lines)?],
    )?;
    Ok(())
}

/// VibeVoice venv layout created by scripts/provision-vibevoice.sh.
pub fn vibevoice_python(data_dir: &Path) -> Option<PathBuf> {
    let py = data_dir.join("vibevoice-venv/bin/python");
    if py.exists() {
        Some(py)
    } else {
        None
    }
}

/// Render every line to data_dir/audio/<date>/seg_NNN.wav via mlx-audio.
/// EXPERIMENTAL: requires the provisioned venv; ~slow, meant for the nightly
/// pregen window. Returns the audio dir on success.
pub async fn render_vibevoice(
    data_dir: &Path,
    date: &str,
    lines: &[ScriptLine],
) -> Result<PathBuf, String> {
    let py = vibevoice_python(data_dir).ok_or("vibevoice venv not provisioned")?;
    let out_dir = data_dir.join("audio").join(date);
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let script = data_dir.join("audio").join(format!("{date}-render.py"));
    let lines_json = serde_json::to_string(lines).map_err(|e| e.to_string())?;
    std::fs::write(
        &script,
        format!(
            r#"
import json, sys
from mlx_audio.tts.generate import generate_audio

lines = json.loads({lines_json:?})
voices = {{"teacher": "af_heart", "student": "am_michael"}}
for i, l in enumerate(lines):
    generate_audio(
        text=l["text"],
        model_path="mlx-community/VibeVoice-1.5B-4bit",
        voice=voices.get(l["speaker"], "af_heart"),
        file_prefix="{out}/seg_%03d" % i,
        audio_format="wav",
        join_audio=True,
        verbose=False,
    )
print("done", len(lines))
"#,
            lines_json = lines_json,
            out = out_dir.to_string_lossy(),
        ),
    )
    .map_err(|e| e.to_string())?;
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(3600),
        tokio::process::Command::new(&py).arg(&script).output(),
    )
    .await
    .map_err(|_| "vibevoice render timed out".to_string())?
    .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(500)
            .collect());
    }
    Ok(out_dir)
}

pub fn mark_rendered(conn: &Connection, course_id: i64, dir: &Path) -> db::Result<()> {
    conn.execute(
        "UPDATE audio_scripts SET engine = 'vibevoice', audio_dir = ?2 WHERE course_id = ?1",
        params![course_id, dir.to_string_lossy()],
    )?;
    Ok(())
}
