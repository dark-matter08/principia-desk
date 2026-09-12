CREATE TABLE principia_exercise_drafts_v1 (
    id INTEGER PRIMARY KEY,
    course_id INTEGER REFERENCES courses(id),
    classroom_session_id INTEGER REFERENCES classroom_sessions(id),
    draft TEXT NOT NULL DEFAULT '',
    completed INTEGER NOT NULL DEFAULT 0,
    reflection TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL,
    UNIQUE(course_id),
    UNIQUE(classroom_session_id)
);
INSERT INTO principia_exercise_drafts_v1
    (id, course_id, draft, completed, reflection, updated_at)
SELECT rowid, course_id, draft, completed, reflection, updated_at FROM exercise_drafts;
DROP TABLE exercise_drafts;
ALTER TABLE principia_exercise_drafts_v1 RENAME TO exercise_drafts;
INSERT INTO exercise_drafts (classroom_session_id, draft, completed, reflection, updated_at)
SELECT id, exercise_draft, exercise_completed, exercise_reflection, COALESCE(completed_at, started_at)
FROM classroom_sessions
WHERE exercise_draft <> '' OR exercise_completed = 1 OR exercise_reflection <> '';
