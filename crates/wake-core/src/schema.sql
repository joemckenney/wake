CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    project_root TEXT,
    shell TEXT,
    metadata TEXT
);

CREATE TABLE IF NOT EXISTS commands (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    started_at TEXT NOT NULL,
    ended_at TEXT,
    command TEXT NOT NULL,
    working_dir TEXT,
    git_branch TEXT,
    exit_code INTEGER,
    duration_ms INTEGER,
    output TEXT,
    output_raw BLOB,
    output_bytes INTEGER,
    truncated BOOLEAN DEFAULT FALSE,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE TABLE IF NOT EXISTS annotations (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    note TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id)
);

CREATE INDEX IF NOT EXISTS idx_commands_session ON commands(session_id);
CREATE INDEX IF NOT EXISTS idx_commands_time ON commands(started_at);
CREATE INDEX IF NOT EXISTS idx_commands_project ON commands(working_dir);
