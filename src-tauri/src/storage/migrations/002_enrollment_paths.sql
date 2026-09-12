-- New enrollment/path records are separate from legacy program preferences.
-- This migration creates no enrollments, schedules, grades or learning credit.
CREATE TABLE course_snapshots (
    fingerprint TEXT PRIMARY KEY CHECK(length(fingerprint) = 64),
    course_id TEXT NOT NULL,
    version TEXT NOT NULL,
    body_json TEXT NOT NULL CHECK(json_valid(body_json)),
    created_at TEXT NOT NULL,
    UNIQUE(fingerprint, course_id)
) STRICT;
CREATE TABLE classes (
    id TEXT PRIMARY KEY,
    course_id TEXT NOT NULL UNIQUE,
    course_snapshot_fingerprint TEXT NOT NULL REFERENCES course_snapshots(fingerprint),
    status TEXT NOT NULL CHECK(status IN ('active','paused','completed')),
    configuration_json TEXT NOT NULL CHECK(json_valid(configuration_json)),
    active_path_revision_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(id, course_id),
    FOREIGN KEY(course_snapshot_fingerprint, course_id) REFERENCES course_snapshots(fingerprint, course_id),
    FOREIGN KEY(active_path_revision_id, id) REFERENCES path_revisions(id, class_id) DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE TABLE enrollment_drafts (
    id TEXT PRIMARY KEY,
    course_id TEXT NOT NULL,
    course_snapshot_fingerprint TEXT NOT NULL REFERENCES course_snapshots(fingerprint),
    configuration_json TEXT NOT NULL CHECK(json_valid(configuration_json)),
    status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','accepted','cancelled')),
    revision INTEGER NOT NULL CHECK(revision > 0),
    accepted_class_id TEXT REFERENCES classes(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK((status = 'accepted' AND accepted_class_id IS NOT NULL) OR (status <> 'accepted' AND accepted_class_id IS NULL)),
    FOREIGN KEY(course_snapshot_fingerprint, course_id) REFERENCES course_snapshots(fingerprint, course_id),
    FOREIGN KEY(accepted_class_id, course_id) REFERENCES classes(id, course_id)
) STRICT;
CREATE UNIQUE INDEX one_pending_enrollment_per_course ON enrollment_drafts(course_id) WHERE status = 'draft';
CREATE TABLE path_revisions (
    id TEXT PRIMARY KEY,
    class_id TEXT NOT NULL REFERENCES classes(id) DEFERRABLE INITIALLY DEFERRED,
    revision INTEGER NOT NULL CHECK(revision > 0),
    course_snapshot_fingerprint TEXT NOT NULL REFERENCES course_snapshots(fingerprint),
    entry_profile_json TEXT NOT NULL CHECK(json_valid(entry_profile_json)),
    plan_json TEXT NOT NULL CHECK(json_valid(plan_json)),
    accepted_at TEXT NOT NULL,
    UNIQUE(class_id, revision),
    UNIQUE(id, class_id)
) STRICT;
CREATE TABLE legacy_crosswalk (
    legacy_table TEXT NOT NULL,
    legacy_key TEXT NOT NULL,
    entity_kind TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    PRIMARY KEY(legacy_table, legacy_key)
) STRICT;
CREATE TRIGGER preserve_course_snapshot BEFORE UPDATE ON course_snapshots BEGIN
    SELECT RAISE(ABORT, 'course snapshots are immutable; create a new version');
END;
CREATE TRIGGER retain_course_snapshot BEFORE DELETE ON course_snapshots BEGIN
    SELECT RAISE(ABORT, 'course snapshots preserve historical curriculum');
END;
CREATE TRIGGER preserve_path_revision BEFORE UPDATE ON path_revisions BEGIN
    SELECT RAISE(ABORT, 'accepted paths are immutable; create a new revision');
END;
CREATE TRIGGER retain_path_revision BEFORE DELETE ON path_revisions BEGIN
    SELECT RAISE(ABORT, 'path revisions preserve learning history');
END;
