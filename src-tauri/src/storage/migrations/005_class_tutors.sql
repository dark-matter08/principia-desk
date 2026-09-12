-- Replace the legacy Claude-only model enum and limited runner list. Session
-- and schedule foreign keys keep their original table name and all row IDs.
CREATE TABLE classroom_programs_v5 (
    subject_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('engineering','language')),
    label TEXT NOT NULL,
    native_label TEXT NOT NULL DEFAULT '',
    short_code TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    agent TEXT NOT NULL CHECK(agent IN (
        'claude','codex','cursor','gemini','deepseek','custom',
        'anthropic','openai','google','openrouter','groq','mistral','ollama',
        'claude-cli','codex-cli','cursor-cli','gemini-cli','custom-cli',
        'deepseek-api','anthropic-api','openai-api','google-api','openrouter-api','groq-api','mistral-api','ollama-api'
    )),
    model TEXT NOT NULL CHECK(length(model) BETWEEN 1 AND 160),
    custom_agent_bin TEXT NOT NULL DEFAULT '',
    prompt_profile TEXT NOT NULL,
    prompt_version TEXT NOT NULL DEFAULT 'v1',
    session_minutes INTEGER NOT NULL DEFAULT 30,
    learning_goal TEXT NOT NULL DEFAULT '',
    target_weekly_minutes INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
INSERT INTO classroom_programs_v5 (
    subject_id,kind,label,native_label,short_code,enabled,agent,model,
    custom_agent_bin,prompt_profile,prompt_version,session_minutes,
    learning_goal,target_weekly_minutes,updated_at
) SELECT subject_id,kind,label,native_label,short_code,enabled,agent,model,
    custom_agent_bin,prompt_profile,prompt_version,session_minutes,
    learning_goal,target_weekly_minutes,updated_at FROM classroom_programs;
DROP TABLE classroom_programs;
ALTER TABLE classroom_programs_v5 RENAME TO classroom_programs;
