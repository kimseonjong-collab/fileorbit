-- FileOrbit local filesystem index, version 1. Apply as one transaction.
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS scan_roots (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    last_completed_run_id TEXT
);
CREATE TABLE IF NOT EXISTS scan_runs (
    id TEXT PRIMARY KEY,
    root_id TEXT NOT NULL REFERENCES scan_roots(id),
    started_at TEXT NOT NULL,
    completed_at TEXT,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    files_seen INTEGER NOT NULL DEFAULT 0 CHECK (files_seen >= 0),
    error TEXT
);
CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY,
    root_id TEXT NOT NULL REFERENCES scan_roots(id),
    path TEXT NOT NULL UNIQUE,
    parent_id TEXT REFERENCES folders(id),
    name TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'present' CHECK (state IN ('present', 'missing')),
    last_seen_run_id TEXT REFERENCES scan_runs(id)
);
CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    root_id TEXT NOT NULL REFERENCES scan_roots(id),
    folder_id TEXT NOT NULL REFERENCES folders(id),
    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    extension TEXT,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    modified_ns INTEGER NOT NULL CHECK (modified_ns >= 0),
    state TEXT NOT NULL DEFAULT 'present' CHECK (state IN ('present', 'missing', 'quarantined')),
    last_seen_run_id TEXT REFERENCES scan_runs(id),
    changed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_files_root_state ON files(root_id, state);
CREATE INDEX IF NOT EXISTS idx_files_name ON files(name);
CREATE INDEX IF NOT EXISTS idx_files_folder ON files(folder_id);
CREATE TABLE IF NOT EXISTS batches (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status TEXT NOT NULL CHECK (status IN ('proposed', 'executing', 'completed', 'partial', 'undone'))
);
CREATE TABLE IF NOT EXISTS proposed_actions (
    id TEXT PRIMARY KEY,
    batch_id TEXT REFERENCES batches(id),
    file_id TEXT REFERENCES files(id),
    kind TEXT NOT NULL CHECK (kind IN ('move', 'create_folder', 'quarantine')),
    source_path TEXT,
    destination_path TEXT NOT NULL,
    reason TEXT,
    status TEXT NOT NULL DEFAULT 'proposed' CHECK (status IN ('proposed', 'approved', 'rejected', 'executed')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS executed_actions (
    id TEXT PRIMARY KEY,
    proposal_id TEXT REFERENCES proposed_actions(id),
    batch_id TEXT NOT NULL REFERENCES batches(id),
    kind TEXT NOT NULL,
    source_path TEXT,
    destination_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('success', 'failed', 'undone', 'undo_failed')),
    undo_source_path TEXT,
    undo_destination_path TEXT,
    executed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    error TEXT
);
CREATE INDEX IF NOT EXISTS idx_executed_batch ON executed_actions(batch_id);
CREATE TABLE IF NOT EXISTS sync_state (
    connector TEXT PRIMARY KEY,
    cursor TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_error TEXT
);
CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    batch_id TEXT REFERENCES batches(id),
    action_id TEXT REFERENCES executed_actions(id),
    event_type TEXT NOT NULL,
    details_json TEXT NOT NULL,
    occurred_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
