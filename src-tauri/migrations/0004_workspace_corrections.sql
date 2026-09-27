-- Imported review data is a proposal record; it cannot execute filesystem commands.
CREATE TABLE workspace_corrections (
    stable_item_id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL REFERENCES files(id),
    correction_revision TEXT NOT NULL,
    user_correction TEXT NOT NULL CHECK (length(trim(user_correction)) > 0),
    normalized_action TEXT NOT NULL CHECK (normalized_action IN ('MOVE', 'HOLD')),
    source_path TEXT NOT NULL,
    destination_path TEXT,
    snapshot_size_bytes INTEGER NOT NULL CHECK (snapshot_size_bytes >= 0),
    snapshot_modified_ns INTEGER NOT NULL CHECK (snapshot_modified_ns >= 0),
    status TEXT NOT NULL DEFAULT 'imported' CHECK (status IN ('imported','conflict','rejected','ready_for_preview')),
    imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_workspace_corrections_file ON workspace_corrections(file_id);
