-- A question bank the tutor wrote for a bundled class, in the diagnostic
-- bank's shape. While a row exists it stands in for the authored bank of
-- that class; restoring the bundled bank deletes the row. A learner's own
-- class keeps its written bank on custom_courses.bank_json.
CREATE TABLE course_banks (
    course_id TEXT PRIMARY KEY,
    bank_json TEXT NOT NULL,
    written_at TEXT NOT NULL
);
