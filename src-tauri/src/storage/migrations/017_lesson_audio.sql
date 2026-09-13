-- Listening mode: the two-host dialogue written for a prepared lesson and
-- where its voiced segments are, one row per study session. Lines are kept
-- even when rendering fails, so the system voice can still read them.
CREATE TABLE lesson_audio (
    session_id TEXT PRIMARY KEY,
    engine TEXT NOT NULL,
    teacher_voice TEXT NOT NULL DEFAULT '',
    student_voice TEXT NOT NULL DEFAULT '',
    lines_json TEXT NOT NULL DEFAULT '[]',
    audio_dir TEXT,
    status TEXT NOT NULL CHECK(status IN ('writing','rendering','ready','failed')),
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
