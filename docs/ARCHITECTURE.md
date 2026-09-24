# FileOrbit Architecture

## Workflow
1. Select an existing work-folder root.
2. Perform a read-only scan.
3. Exclude development/cache/temp/system noise.
4. Build folder profiles from structure and file metadata.
5. Detect ambiguous, duplicate-like, and overlapping folders.
6. Present restructuring recommendations as Dry Run only.
7. After user approval, persist the accepted structure as Folder Map.
8. Scan Downloads only on demand.
9. Match new/changed Downloads files against Folder Map.
10. Preview; move only after explicit approval; record undo journal.

## V0.1 boundary
No move, rename, delete, background watcher, startup task, scheduled task, or cloud database.

## Local data
SQLite stores folder profiles, scan snapshots, rules, and later undo journals. File contents are not uploaded to cloud services.
