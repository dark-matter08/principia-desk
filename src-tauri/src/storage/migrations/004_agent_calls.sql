-- Native counterpart of Remote Ledger's llm_calls, with explicit unknown usage
-- and a pre-dispatch record so an interrupted process does not look successful.
CREATE TABLE agent_calls (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    runner TEXT NOT NULL,
    model TEXT NOT NULL,
    purpose TEXT NOT NULL,
    owner_key TEXT,
    fallback_of TEXT REFERENCES agent_calls(id),
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
    input_tokens INTEGER,
    output_tokens INTEGER,
    cached_tokens INTEGER,
    tokens_estimated INTEGER NOT NULL DEFAULT 0 CHECK (tokens_estimated IN (0,1)),
    cost_usd REAL CHECK (cost_usd IS NULL OR cost_usd >= 0),
    metered INTEGER NOT NULL CHECK (metered IN (0,1)),
    duration_ms INTEGER,
    error_kind TEXT
);
CREATE INDEX agent_calls_month ON agent_calls(started_at, metered);
CREATE INDEX agent_calls_owner ON agent_calls(owner_key, started_at);
