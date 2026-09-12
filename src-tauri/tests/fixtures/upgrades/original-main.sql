-- Schema authored at 73dfa512b9da82d76e453e6dcb0d72aa1db21554:src-tauri/src/db.rs
CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS concepts (
    id INTEGER PRIMARY KEY,
    slug TEXT UNIQUE NOT NULL,
    title TEXT NOT NULL,
    category TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    times_picked INTEGER NOT NULL DEFAULT 0,
    last_picked_date TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    tier INTEGER NOT NULL DEFAULT 0,
    prereqs_json TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE IF NOT EXISTS sessions (
    date TEXT PRIMARY KEY,
    concept_id INTEGER REFERENCES concepts(id),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending','in_progress','completed','skipped')),
    current_step TEXT NOT NULL DEFAULT 'quiz',
    quiz_score REAL,
    started_at TEXT,
    completed_at TEXT,
    reading_seconds INTEGER NOT NULL DEFAULT 0,
    session_type TEXT NOT NULL DEFAULT 'lesson',
    plan_reason TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS courses (
    id INTEGER PRIMARY KEY,
    session_date TEXT NOT NULL,
    concept_id INTEGER NOT NULL REFERENCES concepts(id),
    markdown TEXT NOT NULL,
    resources_json TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL CHECK(source IN ('claude','codex','fallback')),
    generated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS questions (
    id INTEGER PRIMARY KEY,
    course_id INTEGER NOT NULL REFERENCES courses(id),
    prompt TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('mcq','free')),
    choices_json TEXT,
    correct_answer TEXT NOT NULL,
    explanation TEXT NOT NULL,
    origin TEXT NOT NULL DEFAULT 'fresh' CHECK(origin IN ('fresh','carryover'))
);
CREATE TABLE IF NOT EXISTS attempts (
    id INTEGER PRIMARY KEY,
    question_id INTEGER NOT NULL REFERENCES questions(id),
    session_date TEXT NOT NULL,
    user_answer TEXT NOT NULL,
    correct INTEGER NOT NULL,
    grader_feedback TEXT NOT NULL DEFAULT '',
    graded_by TEXT NOT NULL DEFAULT 'local'
);
CREATE TABLE IF NOT EXISTS carryover (
    question_id INTEGER PRIMARY KEY REFERENCES questions(id),
    failed_on TEXT NOT NULL,
    times_failed INTEGER NOT NULL DEFAULT 1,
    scheduled_for TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS audio_scripts (
    course_id INTEGER PRIMARY KEY REFERENCES courses(id),
    lines_json TEXT NOT NULL,
    engine TEXT NOT NULL DEFAULT 'speech',  -- 'speech' (webview TTS) | 'vibevoice' (rendered files)
    audio_dir TEXT,                          -- set when files are rendered
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS exit_questions (
    id INTEGER PRIMARY KEY,
    course_id INTEGER NOT NULL REFERENCES courses(id),
    prompt TEXT NOT NULL,
    choices_json TEXT NOT NULL,
    correct_answer TEXT NOT NULL,
    explanation TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS mastery (
    concept_id INTEGER PRIMARY KEY REFERENCES concepts(id),
    state TEXT NOT NULL DEFAULT 'unseen'
        CHECK(state IN ('unseen','introduced','practicing','struggling','mastered','maintenance','decayed')),
    score_ema REAL NOT NULL DEFAULT 0,
    encounters INTEGER NOT NULL DEFAULT 0,
    last_seen_date TEXT,
    next_review_date TEXT,
    review_interval_days INTEGER NOT NULL DEFAULT 7,
    teacher_notes TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS profile (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS generation_jobs (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('course','quiz')),
    target_date TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK(status IN ('queued','running','done','failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    created_at TEXT NOT NULL,
    finished_at TEXT,
    UNIQUE(kind, target_date)
);
