-- Stable identities for the primary compatibility adapter. The FSM cutover
-- reuses these keys; service dates remain legacy lookup/provenance only.
CREATE TABLE primary_session_ids (
    session_id TEXT PRIMARY KEY,
    legacy_date TEXT NOT NULL UNIQUE REFERENCES sessions(date)
) STRICT;
INSERT INTO primary_session_ids SELECT 'primary-' || lower(hex(randomblob(16))), date FROM sessions;
INSERT INTO legacy_crosswalk(legacy_table,legacy_key,entity_kind,entity_id,imported_at)
    SELECT 'sessions',legacy_date,'primary_session',session_id,strftime('%Y-%m-%dT%H:%M:%fZ','now') FROM primary_session_ids;
CREATE TRIGGER assign_primary_session_id AFTER INSERT ON sessions BEGIN
    INSERT INTO primary_session_ids VALUES('primary-' || lower(hex(randomblob(16))),NEW.date);
    INSERT INTO legacy_crosswalk(legacy_table,legacy_key,entity_kind,entity_id,imported_at)
        SELECT 'sessions',NEW.date,'primary_session',session_id,strftime('%Y-%m-%dT%H:%M:%fZ','now')
        FROM primary_session_ids WHERE legacy_date=NEW.date;
END;
CREATE TRIGGER immutable_primary_identity BEFORE UPDATE ON primary_session_ids BEGIN
    SELECT RAISE(ABORT, 'primary session identities cannot change');
END;
CREATE TRIGGER retain_primary_identity BEFORE DELETE ON primary_session_ids BEGIN
    SELECT RAISE(ABORT, 'primary session identities preserve saved ownership');
END;
