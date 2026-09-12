-- Durable appointments: one occurrence per rule per local service date, with
-- the rule revision, intended local time and resolved instant snapshotted.
-- Rules keep their table and IDs; edits bump a revision.
ALTER TABLE classroom_schedule_slots ADD COLUMN revision INTEGER NOT NULL DEFAULT 1;
CREATE TABLE schedule_occurrences (
    id TEXT PRIMARY KEY,
    rule_id INTEGER REFERENCES classroom_schedule_slots(id) ON DELETE SET NULL,
    course_id TEXT NOT NULL,
    rule_revision INTEGER NOT NULL,
    local_date TEXT NOT NULL,
    local_time TEXT NOT NULL,
    timezone TEXT NOT NULL,
    fires_at TEXT NOT NULL,
    duration_minutes INTEGER NOT NULL,
    disposition TEXT NOT NULL CHECK(disposition IN ('scheduled','due','started','completed','skipped','missed')),
    session_ref TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    resolved_at TEXT,
    CHECK((disposition IN ('started','completed','skipped','missed')) = (resolved_at IS NOT NULL)),
    UNIQUE(rule_id, local_date)
) STRICT;
CREATE INDEX occurrences_by_course_date ON schedule_occurrences(course_id, local_date);
CREATE INDEX occurrences_by_session ON schedule_occurrences(session_ref);
CREATE TRIGGER immutable_occurrence_snapshot BEFORE UPDATE OF id, course_id, local_date, created_at ON schedule_occurrences BEGIN
    SELECT RAISE(ABORT, 'appointment identity and service date are immutable');
END;
CREATE TRIGGER final_occurrence_disposition BEFORE UPDATE OF disposition ON schedule_occurrences
WHEN OLD.disposition IN ('completed','skipped') BEGIN
    SELECT RAISE(ABORT, 'a finished appointment is consumed exactly once');
END;
