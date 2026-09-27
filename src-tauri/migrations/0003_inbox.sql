-- Review state is separate from filesystem metadata; discovery never executes an action.
CREATE TABLE inbox_items (
    file_id TEXT PRIMARY KEY REFERENCES files(id),
    review_state TEXT NOT NULL DEFAULT 'new' CHECK (review_state IN ('new','reviewing','held','proposed')),
    related_state TEXT NOT NULL DEFAULT 'pending' CHECK (related_state IN ('pending','ready')),
    proposed_destination TEXT,
    reason TEXT,
    confidence REAL CHECK (confidence IS NULL OR confidence BETWEEN 0 AND 1),
    action_status TEXT NOT NULL DEFAULT 'none' CHECK (action_status IN ('none','proposed','approved','rejected')),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_inbox_review ON inbox_items(review_state);
