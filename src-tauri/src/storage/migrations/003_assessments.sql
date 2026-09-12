-- Shared assessment storage. Primary quizzes cut over first; diagnostics and
-- subject adapters reuse the same immutable rounds and versioned answer drafts.
CREATE TABLE assessment_attempts (
    id TEXT PRIMARY KEY,
    owner_kind TEXT NOT NULL CHECK(owner_kind IN ('legacy_primary','enrollment_draft','class')),
    owner_key TEXT NOT NULL,
    purpose TEXT NOT NULL CHECK(purpose IN ('retrieval','diagnostic','unit_challenge','exit_check','lesson')),
    context_json TEXT NOT NULL CHECK(json_valid(context_json)),
    status TEXT NOT NULL CHECK(status IN ('active','completed','abandoned')),
    created_at TEXT NOT NULL,
    finished_at TEXT,
    CHECK((status = 'active') = (finished_at IS NULL))
) STRICT;
CREATE UNIQUE INDEX one_active_assessment_per_owner
    ON assessment_attempts(owner_kind, owner_key) WHERE status = 'active';
CREATE INDEX assessments_by_owner ON assessment_attempts(owner_kind, owner_key, purpose);
CREATE TABLE assessment_rounds (
    id TEXT PRIMARY KEY,
    attempt_id TEXT NOT NULL REFERENCES assessment_attempts(id),
    ordinal INTEGER NOT NULL CHECK(ordinal > 0),
    rubric_version TEXT NOT NULL,
    items_json TEXT NOT NULL CHECK(json_valid(items_json) AND json_type(items_json) = 'array'),
    created_at TEXT NOT NULL,
    UNIQUE(attempt_id, ordinal)
) STRICT;
CREATE TABLE assessment_work (
    round_id TEXT PRIMARY KEY REFERENCES assessment_rounds(id),
    revision INTEGER NOT NULL CHECK(revision >= 0),
    responses_json TEXT NOT NULL CHECK(json_valid(responses_json) AND json_type(responses_json) = 'object'),
    updated_at TEXT NOT NULL
) STRICT;
CREATE TABLE assessment_submissions (
    round_id TEXT PRIMARY KEY REFERENCES assessment_rounds(id),
    answer_revision INTEGER NOT NULL CHECK(answer_revision >= 0),
    responses_json TEXT NOT NULL CHECK(json_valid(responses_json)),
    result_json TEXT NOT NULL CHECK(json_valid(result_json)),
    submitted_at TEXT NOT NULL
) STRICT;

-- Preserve original bytes, including formats that cannot be reconstructed.
-- Import into typed rounds once, through the primary adapter. Original config
-- rows remain historical records; no new quiz writes use them after cutover.
CREATE TABLE legacy_assessment_config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;
INSERT INTO legacy_assessment_config SELECT key, value FROM config
WHERE key GLOB 'quiz_round:*' OR key GLOB 'quiz_result:*' OR key GLOB 'pending_answers:*';
CREATE TRIGGER retired_quiz_config_insert BEFORE INSERT ON config
WHEN NEW.key GLOB 'quiz_round:*' OR NEW.key GLOB 'quiz_result:*' OR NEW.key GLOB 'pending_answers:*' BEGIN
    SELECT RAISE(ABORT, 'quiz writes must use the shared assessment runtime');
END;
CREATE TRIGGER retired_quiz_config_update BEFORE UPDATE ON config
WHEN OLD.key GLOB 'quiz_round:*' OR OLD.key GLOB 'quiz_result:*' OR OLD.key GLOB 'pending_answers:*'
  OR NEW.key GLOB 'quiz_round:*' OR NEW.key GLOB 'quiz_result:*' OR NEW.key GLOB 'pending_answers:*' BEGIN
    SELECT RAISE(ABORT, 'legacy quiz settings are read-only');
END;
CREATE TRIGGER retain_quiz_config BEFORE DELETE ON config
WHEN OLD.key GLOB 'quiz_round:*' OR OLD.key GLOB 'quiz_result:*' OR OLD.key GLOB 'pending_answers:*' BEGIN
    SELECT RAISE(ABORT, 'legacy quiz settings preserve history');
END;

CREATE TRIGGER immutable_assessment_round BEFORE UPDATE ON assessment_rounds BEGIN
    SELECT RAISE(ABORT, 'assessment rounds are immutable');
END;
CREATE TRIGGER retain_assessment_round BEFORE DELETE ON assessment_rounds BEGIN
    SELECT RAISE(ABORT, 'assessment rounds preserve submitted and saved work');
END;
CREATE TRIGGER immutable_assessment_submission BEFORE UPDATE ON assessment_submissions BEGIN
    SELECT RAISE(ABORT, 'assessment submissions are immutable');
END;
CREATE TRIGGER retain_assessment_submission BEFORE DELETE ON assessment_submissions BEGIN
    SELECT RAISE(ABORT, 'assessment submissions preserve evidence');
END;
CREATE TRIGGER immutable_assessment_context BEFORE UPDATE OF owner_kind, owner_key, purpose, context_json, created_at ON assessment_attempts BEGIN
    SELECT RAISE(ABORT, 'assessment ownership and context are immutable');
END;
CREATE TRIGGER immutable_terminal_assessment BEFORE UPDATE ON assessment_attempts
WHEN OLD.status <> 'active' BEGIN
    SELECT RAISE(ABORT, 'terminal assessment attempts are immutable');
END;
CREATE TRIGGER immutable_submitted_work BEFORE UPDATE ON assessment_work
WHEN EXISTS(SELECT 1 FROM assessment_submissions WHERE round_id = OLD.round_id) BEGIN
    SELECT RAISE(ABORT, 'submitted answers are immutable');
END;
CREATE TRIGGER retain_assessment_work BEFORE DELETE ON assessment_work BEGIN
    SELECT RAISE(ABORT, 'assessment work is retained');
END;
CREATE TRIGGER immutable_legacy_assessment BEFORE UPDATE ON legacy_assessment_config BEGIN
    SELECT RAISE(ABORT, 'legacy assessment snapshots are immutable');
END;
CREATE TRIGGER retain_legacy_assessment BEFORE DELETE ON legacy_assessment_config BEGIN
    SELECT RAISE(ABORT, 'legacy assessment snapshots are retained');
END;
