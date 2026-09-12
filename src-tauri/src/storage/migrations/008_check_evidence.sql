-- Knowledge-check evidence for shared-runtime lessons. Legacy classroom rows
-- keep their session link and IDs; new rows reference a study session instead.
CREATE TABLE principia_classroom_exit_attempts_v8 (
    id INTEGER PRIMARY KEY,
    session_id INTEGER REFERENCES classroom_sessions(id) ON DELETE CASCADE,
    study_session_id TEXT REFERENCES study_sessions(id),
    concept_id INTEGER NOT NULL REFERENCES concepts(id) ON DELETE CASCADE,
    question_id INTEGER NOT NULL,
    section TEXT NOT NULL DEFAULT '',
    learning_objective TEXT NOT NULL DEFAULT '',
    misconception TEXT NOT NULL DEFAULT '',
    correct INTEGER NOT NULL DEFAULT 0,
    attempted_at TEXT NOT NULL,
    CHECK((session_id IS NULL) <> (study_session_id IS NULL)),
    UNIQUE(session_id, question_id),
    UNIQUE(study_session_id, question_id)
);
INSERT INTO principia_classroom_exit_attempts_v8
    (id, session_id, concept_id, question_id, section, learning_objective, misconception, correct, attempted_at)
SELECT id, session_id, concept_id, question_id, section, learning_objective, misconception, correct, attempted_at
FROM classroom_exit_attempts;
DROP TABLE classroom_exit_attempts;
ALTER TABLE principia_classroom_exit_attempts_v8 RENAME TO classroom_exit_attempts;
CREATE INDEX idx_classroom_exit_attempts_concept ON classroom_exit_attempts(concept_id, correct, id);
CREATE INDEX idx_classroom_exit_attempts_study ON classroom_exit_attempts(study_session_id);
