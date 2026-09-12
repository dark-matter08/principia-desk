CREATE TABLE principia_courses_v1 (
    id INTEGER PRIMARY KEY,
    session_date TEXT NOT NULL,
    concept_id INTEGER NOT NULL REFERENCES concepts(id),
    markdown TEXT NOT NULL,
    resources_json TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL CHECK(source IN ('claude','codex','cursor','gemini','deepseek','custom','fallback')),
    generated_at TEXT NOT NULL
);
INSERT INTO principia_courses_v1
    (id, session_date, concept_id, markdown, resources_json, source, generated_at)
SELECT id, session_date, concept_id, markdown, resources_json, source, generated_at FROM courses;
DROP TABLE courses;
ALTER TABLE principia_courses_v1 RENAME TO courses;
