-- A learner's own classes. The draft is what the builder edits; the
-- definition and curriculum are what was last published and what the
-- runtime registers at startup. Topics of a published course live in
-- `concepts` under the course id, like the bundled seed.
CREATE TABLE custom_courses (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL CHECK (status IN ('draft', 'published')),
    origin TEXT NOT NULL CHECK (origin IN ('tutor', 'manual', 'import')),
    brief_json TEXT NOT NULL,
    draft_json TEXT NOT NULL,
    definition_json TEXT,
    curriculum_json TEXT,
    prompt TEXT,
    review_json TEXT,
    sources_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    published_at TEXT
) STRICT;
