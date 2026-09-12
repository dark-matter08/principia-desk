-- Schema authored at 3cc544e:src-tauri/src/db.rs
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
    prereqs_json TEXT NOT NULL DEFAULT '[]',
    focus TEXT NOT NULL DEFAULT 'system-design',
    brief_json TEXT NOT NULL DEFAULT '{}'
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
    plan_reason TEXT NOT NULL DEFAULT '',
    focus TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS courses (
    id INTEGER PRIMARY KEY,
    session_date TEXT NOT NULL,
    concept_id INTEGER NOT NULL REFERENCES concepts(id),
    markdown TEXT NOT NULL,
    resources_json TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL CHECK(source IN ('claude','codex','cursor','gemini','deepseek','custom','fallback')),
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
    round INTEGER NOT NULL DEFAULT 1,
    prompt TEXT NOT NULL,
    choices_json TEXT NOT NULL,
    correct_answer TEXT NOT NULL,
    explanation TEXT NOT NULL DEFAULT '',
    section TEXT NOT NULL DEFAULT '',
    learning_objective TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS exit_attempts (
    id INTEGER PRIMARY KEY,
    course_id INTEGER NOT NULL REFERENCES courses(id),
    question_id INTEGER NOT NULL REFERENCES exit_questions(id),
    round INTEGER NOT NULL,
    correct INTEGER NOT NULL,
    user_answer TEXT NOT NULL,
    section TEXT NOT NULL DEFAULT '',
    learning_objective TEXT NOT NULL DEFAULT '',
    misconception TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS course_exercises (
    course_id INTEGER PRIMARY KEY REFERENCES courses(id),
    title TEXT NOT NULL,
    instructions TEXT NOT NULL,
    starter_code TEXT,
    deliverable TEXT,
    hints_json TEXT NOT NULL DEFAULT '[]'
);
CREATE TABLE IF NOT EXISTS exercise_drafts (
    course_id INTEGER PRIMARY KEY REFERENCES courses(id),
    draft TEXT NOT NULL DEFAULT '',
    completed INTEGER NOT NULL DEFAULT 0,
    reflection TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL
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
CREATE TABLE IF NOT EXISTS language_programs (
    language TEXT PRIMARY KEY CHECK(language IN ('german','italian')),
    enabled INTEGER NOT NULL DEFAULT 0,
    start_level TEXT NOT NULL DEFAULT 'A1'
        CHECK(start_level IN ('A1','A2','B1','B2')),
    current_level TEXT NOT NULL DEFAULT 'A1'
        CHECK(current_level IN ('A1','A2','B1','B2')),
    target_level TEXT NOT NULL DEFAULT 'A2'
        CHECK(target_level IN ('A1','A2','B1','B2')),
    start_date TEXT NOT NULL,
    weekly_minutes INTEGER NOT NULL DEFAULT 210,
    session_minutes INTEGER NOT NULL DEFAULT 30,
    preferred INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS language_schedule_slots (
    id INTEGER PRIMARY KEY,
    language TEXT NOT NULL REFERENCES language_programs(language) ON DELETE CASCADE,
    hour INTEGER NOT NULL CHECK(hour BETWEEN 0 AND 23),
    minute INTEGER NOT NULL CHECK(minute BETWEEN 0 AND 59),
    weekdays_json TEXT NOT NULL DEFAULT '[1,2,3,4,5,6,7]',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    UNIQUE(language, hour, minute)
);
CREATE TABLE IF NOT EXISTS language_sessions (
    id INTEGER PRIMARY KEY,
    slot_id INTEGER REFERENCES language_schedule_slots(id) ON DELETE SET NULL,
    classroom_slot_id INTEGER,
    language TEXT NOT NULL REFERENCES language_programs(language) ON DELETE CASCADE,
    session_date TEXT NOT NULL,
    level TEXT NOT NULL CHECK(level IN ('A1','A2','B1','B2')),
    unit_slug TEXT NOT NULL,
    phase INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'in_progress'
        CHECK(status IN ('in_progress','completed','skipped')),
    lesson_json TEXT NOT NULL,
    score REAL,
    response_json TEXT NOT NULL DEFAULT '{}',
    started_at TEXT NOT NULL,
    completed_at TEXT,
    UNIQUE(slot_id, session_date)
);
CREATE INDEX IF NOT EXISTS idx_language_sessions_program_date
    ON language_sessions(language, session_date, status);
CREATE TABLE IF NOT EXISTS language_unit_progress (
    language TEXT NOT NULL REFERENCES language_programs(language) ON DELETE CASCADE,
    unit_slug TEXT NOT NULL,
    phase_completed INTEGER NOT NULL DEFAULT 0,
    score_ema REAL NOT NULL DEFAULT 0,
    encounters INTEGER NOT NULL DEFAULT 0,
    last_seen_date TEXT,
    next_review_date TEXT,
    PRIMARY KEY(language, unit_slug)
);
CREATE TABLE IF NOT EXISTS language_skill_scores (
    language TEXT NOT NULL REFERENCES language_programs(language) ON DELETE CASCADE,
    strand TEXT NOT NULL,
    score_ema REAL NOT NULL DEFAULT 0,
    encounters INTEGER NOT NULL DEFAULT 0,
    last_seen_date TEXT,
    PRIMARY KEY(language, strand)
);
CREATE TABLE IF NOT EXISTS classroom_programs (
    subject_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('engineering','language')),
    label TEXT NOT NULL,
    native_label TEXT NOT NULL DEFAULT '',
    short_code TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    agent TEXT NOT NULL
        CHECK(agent IN ('claude','codex','cursor','gemini','deepseek','custom')),
    model TEXT NOT NULL CHECK(model IN ('opus','sonnet','haiku')),
    custom_agent_bin TEXT NOT NULL DEFAULT '',
    prompt_profile TEXT NOT NULL,
    prompt_version TEXT NOT NULL DEFAULT 'v1',
    session_minutes INTEGER NOT NULL DEFAULT 30,
    learning_goal TEXT NOT NULL DEFAULT '',
    target_weekly_minutes INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS classroom_schedule_slots (
    id INTEGER PRIMARY KEY,
    subject_id TEXT NOT NULL REFERENCES classroom_programs(subject_id) ON DELETE CASCADE,
    hour INTEGER NOT NULL CHECK(hour BETWEEN 0 AND 23),
    minute INTEGER NOT NULL CHECK(minute BETWEEN 0 AND 59),
    weekdays_json TEXT NOT NULL DEFAULT '[1,2,3,4,5,6,7]',
    enabled INTEGER NOT NULL DEFAULT 1,
    source TEXT NOT NULL DEFAULT 'manual' CHECK(source IN ('manual','planned')),
    created_at TEXT NOT NULL,
    UNIQUE(subject_id, hour, minute)
);
CREATE TABLE IF NOT EXISTS classroom_sessions (
    id INTEGER PRIMARY KEY,
    subject_id TEXT NOT NULL REFERENCES classroom_programs(subject_id) ON DELETE CASCADE,
    slot_id INTEGER REFERENCES classroom_schedule_slots(id) ON DELETE SET NULL,
    session_date TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'in_progress'
        CHECK(status IN ('in_progress','completed','skipped')),
    title TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    score REAL,
    response_json TEXT NOT NULL DEFAULT '{}',
    exercise_draft TEXT NOT NULL DEFAULT '',
    exercise_completed INTEGER NOT NULL DEFAULT 0,
    exercise_reflection TEXT NOT NULL DEFAULT '',
    agent_used TEXT NOT NULL DEFAULT 'fallback',
    prompt_version TEXT NOT NULL,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    UNIQUE(slot_id, session_date)
);
CREATE INDEX IF NOT EXISTS idx_classroom_sessions_subject_date
    ON classroom_sessions(subject_id, session_date, status);
CREATE TABLE IF NOT EXISTS classroom_exit_attempts (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES classroom_sessions(id) ON DELETE CASCADE,
    concept_id INTEGER NOT NULL REFERENCES concepts(id) ON DELETE CASCADE,
    question_id INTEGER NOT NULL,
    section TEXT NOT NULL DEFAULT '',
    learning_objective TEXT NOT NULL DEFAULT '',
    misconception TEXT NOT NULL DEFAULT '',
    correct INTEGER NOT NULL DEFAULT 0,
    attempted_at TEXT NOT NULL,
    UNIQUE(session_id, question_id)
);
CREATE INDEX IF NOT EXISTS idx_classroom_exit_attempts_concept
    ON classroom_exit_attempts(concept_id, correct, id);
