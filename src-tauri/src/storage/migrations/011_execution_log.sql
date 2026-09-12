-- Every line the tutor's runner reports while preparing a lesson, kept so a
-- run can be reviewed after the fact. A run is one preparation of one
-- session; lines outside any preparation carry no run.
CREATE TABLE execution_log_lines (
    id INTEGER PRIMARY KEY,
    at TEXT NOT NULL,
    run_id TEXT,
    course_id TEXT,
    line TEXT NOT NULL
) STRICT;
CREATE INDEX execution_log_by_run ON execution_log_lines(run_id, id);
CREATE INDEX execution_log_by_time ON execution_log_lines(at);
