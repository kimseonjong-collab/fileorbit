# Phase 2 SQLite Index — schema slice

- Migration `0001_index.sql` defines files, folders, scan roots/runs, proposals, executions, batches, undo fields, sync state and audit events.
- `scripts/test-index-schema.py` uses only disposable temporary SQLite databases. It checks repeatable creation, synthetic scan rows, constraints, and rollback of a failed write transaction.
- This slice does not open, create or modify `C:\FileOrbit\data\fileorbit.db`.
- **Pending:** wire the migration into the Tauri backend using a pinned SQLite dependency and update `Cargo.lock`; implement database lifecycle, transactional migration and indexed scan persistence. The SQL fixture test alone is not Phase 2 acceptance.
