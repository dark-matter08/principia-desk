-- Shared runtime foundation. Existing engine records are retained until each
-- adapter switches its writer, due/consumed reads and history in one cutover.
CREATE UNIQUE INDEX path_snapshot_owner ON path_revisions(id, class_id, course_snapshot_fingerprint);

CREATE TABLE study_sessions (
    id TEXT PRIMARY KEY,
    request_key TEXT NOT NULL UNIQUE,
    request_json TEXT NOT NULL CHECK(json_valid(request_json)),
    owner_kind TEXT NOT NULL CHECK(owner_kind IN ('class','daily_routine')),
    owner_key TEXT NOT NULL,
    class_id TEXT REFERENCES classes(id),
    course_snapshot_fingerprint TEXT NOT NULL REFERENCES course_snapshots(fingerprint),
    path_revision_id TEXT,
    context_json TEXT NOT NULL CHECK(json_valid(context_json)),
    status TEXT NOT NULL CHECK(status IN ('planned','preparing','ready','active','paused','completed','skipped')),
    lesson_version_id TEXT,
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    finished_at TEXT,
    CHECK((owner_kind='class' AND class_id IS NOT NULL AND owner_key=class_id AND path_revision_id IS NOT NULL)
        OR (owner_kind='daily_routine' AND owner_key='daily-routine' AND class_id IS NULL AND path_revision_id IS NULL)),
    CHECK((status IN ('completed','skipped')) = (finished_at IS NOT NULL)),
    CHECK(status NOT IN ('ready','active','paused','completed') OR lesson_version_id IS NOT NULL),
    FOREIGN KEY(path_revision_id, class_id, course_snapshot_fingerprint)
        REFERENCES path_revisions(id, class_id, course_snapshot_fingerprint),
    FOREIGN KEY(lesson_version_id, id) REFERENCES lesson_versions(id, session_id)
) STRICT;
CREATE UNIQUE INDEX one_resumable_study_session_per_owner ON study_sessions(owner_kind, owner_key)
    WHERE status NOT IN ('completed','skipped');
CREATE UNIQUE INDEX one_foreground_study_session ON study_sessions((1)) WHERE status='active';
CREATE INDEX study_history_by_class ON study_sessions(class_id, created_at);

CREATE TABLE lesson_versions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE REFERENCES study_sessions(id),
    content_json TEXT NOT NULL CHECK(json_valid(content_json)),
    fingerprint TEXT NOT NULL CHECK(length(fingerprint)=64),
    created_at TEXT NOT NULL,
    UNIQUE(id, session_id)
) STRICT;
CREATE TABLE study_preparation_jobs (
    session_id TEXT PRIMARY KEY REFERENCES study_sessions(id),
    request_fingerprint TEXT NOT NULL CHECK(length(request_fingerprint)=64),
    status TEXT NOT NULL CHECK(status IN ('queued','running','failed','ready','cancelled')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0),
    lease_token TEXT,
    lease_expires_at INTEGER,
    finished_token TEXT,
    error TEXT,
    updated_at TEXT NOT NULL,
    CHECK((status='running') = (lease_token IS NOT NULL AND lease_expires_at IS NOT NULL)),
    CHECK(status='running' OR (lease_token IS NULL AND lease_expires_at IS NULL)),
    CHECK(status<>'failed' OR error IS NOT NULL)
) STRICT;
CREATE TABLE study_checkpoints (
    session_id TEXT PRIMARY KEY REFERENCES study_sessions(id),
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0),
    body_json TEXT NOT NULL CHECK(json_valid(body_json)),
    updated_at TEXT NOT NULL
) STRICT;
CREATE TABLE study_results (
    session_id TEXT PRIMARY KEY REFERENCES study_sessions(id),
    disposition TEXT NOT NULL CHECK(disposition IN ('completed','skipped')),
    checkpoint_revision INTEGER NOT NULL,
    checkpoint_json TEXT NOT NULL CHECK(json_valid(checkpoint_json)),
    outcome_json TEXT NOT NULL CHECK(json_valid(outcome_json)),
    finished_at TEXT NOT NULL
) STRICT;

CREATE TRIGGER immutable_study_context BEFORE UPDATE OF id, request_key, request_json, owner_kind, owner_key, class_id, course_snapshot_fingerprint, path_revision_id, context_json, created_at ON study_sessions BEGIN
    SELECT RAISE(ABORT, 'session ownership and preparation context are immutable');
END;
CREATE TRIGGER immutable_terminal_study_session BEFORE UPDATE ON study_sessions
WHEN OLD.status IN ('completed','skipped') BEGIN
    SELECT RAISE(ABORT, 'terminal sessions are immutable');
END;
CREATE TRIGGER valid_study_transition BEFORE UPDATE OF status ON study_sessions
WHEN NEW.status<>OLD.status AND NOT (
    (OLD.status='planned' AND NEW.status IN ('preparing','skipped')) OR
    (OLD.status='preparing' AND NEW.status IN ('ready','skipped')) OR
    (OLD.status='ready' AND NEW.status IN ('active','skipped')) OR
    (OLD.status='active' AND NEW.status IN ('paused','completed','skipped')) OR
    (OLD.status='paused' AND NEW.status IN ('active','skipped'))
) BEGIN
    SELECT RAISE(ABORT, 'invalid study session transition');
END;
CREATE TRIGGER bind_lesson_once BEFORE UPDATE OF lesson_version_id ON study_sessions
WHEN OLD.lesson_version_id IS NOT NULL OR NEW.lesson_version_id IS NULL OR OLD.status<>'preparing' OR NEW.status<>'ready' BEGIN
    SELECT RAISE(ABORT, 'a prepared lesson can only be bound once');
END;
CREATE TRIGGER retain_study_session BEFORE DELETE ON study_sessions BEGIN
    SELECT RAISE(ABORT, 'study sessions preserve saved work and history');
END;
CREATE TRIGGER immutable_lesson_version BEFORE UPDATE ON lesson_versions BEGIN
    SELECT RAISE(ABORT, 'lesson versions are immutable');
END;
CREATE TRIGGER retain_lesson_version BEFORE DELETE ON lesson_versions BEGIN
    SELECT RAISE(ABORT, 'lesson versions preserve original content');
END;
CREATE TRIGGER immutable_preparation_request BEFORE UPDATE OF session_id, request_fingerprint ON study_preparation_jobs BEGIN
    SELECT RAISE(ABORT, 'preparation requests belong to their original session');
END;
CREATE TRIGGER immutable_finished_preparation BEFORE UPDATE ON study_preparation_jobs
WHEN OLD.status IN ('ready','cancelled') BEGIN
    SELECT RAISE(ABORT, 'finished preparation jobs cannot be reopened');
END;
CREATE TRIGGER immutable_terminal_checkpoint BEFORE UPDATE ON study_checkpoints
WHEN EXISTS(SELECT 1 FROM study_sessions WHERE id=OLD.session_id AND status IN ('completed','skipped')) BEGIN
    SELECT RAISE(ABORT, 'terminal session work is immutable');
END;
CREATE TRIGGER immutable_checkpoint_owner BEFORE UPDATE OF session_id ON study_checkpoints BEGIN
    SELECT RAISE(ABORT, 'saved work cannot change owner');
END;
CREATE TRIGGER retain_study_checkpoint BEFORE DELETE ON study_checkpoints BEGIN
    SELECT RAISE(ABORT, 'saved session work is retained');
END;
CREATE TRIGGER immutable_study_result BEFORE UPDATE ON study_results BEGIN
    SELECT RAISE(ABORT, 'session results are immutable');
END;
CREATE TRIGGER retain_study_result BEFORE DELETE ON study_results BEGIN
    SELECT RAISE(ABORT, 'session results preserve submitted evidence');
END;

-- Extend the existing assessment runtime with session ownership. Preserve every
-- attempt, frozen round, answer draft and submission; do not copy grading rules.
CREATE TABLE principia_assessment_attempts_v6 (
    id TEXT PRIMARY KEY,
    owner_kind TEXT NOT NULL CHECK(owner_kind IN ('legacy_primary','enrollment_draft','class','study_session')),
    owner_key TEXT NOT NULL,
    purpose TEXT NOT NULL CHECK(purpose IN ('retrieval','diagnostic','unit_challenge','exit_check','lesson')),
    context_json TEXT NOT NULL CHECK(json_valid(context_json)),
    status TEXT NOT NULL CHECK(status IN ('active','completed','abandoned')),
    created_at TEXT NOT NULL,
    finished_at TEXT,
    CHECK((status='active') = (finished_at IS NULL))
) STRICT;
INSERT INTO principia_assessment_attempts_v6 SELECT * FROM assessment_attempts;
DROP TABLE assessment_attempts;
ALTER TABLE principia_assessment_attempts_v6 RENAME TO assessment_attempts;
CREATE UNIQUE INDEX one_active_assessment_per_owner ON assessment_attempts(owner_kind, owner_key) WHERE status='active';
CREATE INDEX assessments_by_owner ON assessment_attempts(owner_kind, owner_key, purpose);
CREATE TRIGGER immutable_assessment_context BEFORE UPDATE OF owner_kind, owner_key, purpose, context_json, created_at ON assessment_attempts BEGIN
    SELECT RAISE(ABORT, 'assessment ownership and context are immutable');
END;
CREATE TRIGGER immutable_terminal_assessment BEFORE UPDATE ON assessment_attempts
WHEN OLD.status<>'active' BEGIN
    SELECT RAISE(ABORT, 'terminal assessment attempts are immutable');
END;
CREATE TRIGGER valid_study_assessment_owner BEFORE INSERT ON assessment_attempts
WHEN NEW.owner_kind='study_session' AND NOT EXISTS(
    SELECT 1 FROM study_sessions WHERE id=NEW.owner_key AND status IN ('ready','active','paused')
) BEGIN
    SELECT RAISE(ABORT, 'assessment requires an available study session');
END;
