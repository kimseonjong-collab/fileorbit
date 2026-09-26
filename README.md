# FileOrbit

Local-first Windows folder analysis and download organization tool.

## Current scope · V0.4
- V0.1 Folder Doctor: explicit work-root selection and read-only analysis
- V0.2 Folder Map: role-based destination map, approval, persistence, drift detection
- V0.3 Downloads Organizer: explicit Downloads scan, classification, review/hold queue, local correction learning
- V0.4 Safe Move & Undo: per-file approval, backend safety validation, journaled move, persistent history/audit, retry-safe Undo
- Cross-volume move fallback uses temporary copy, size verification, final rename, source removal, and rollback on source-removal failure; fallback behavior is covered by Rust regression tests
- No background file moves, no scheduled file moves, no automatic deletion, no silent overwrite

## Safety baseline
Real file moves require an approved Folder Map, explicit per-file approval, a final Move gate, and backend validation. Validation is invalidated whenever the review/classification set or Downloads scan changes, and final execution is bound to the exact validated source-target pairs. Work and Downloads roots must be separate sibling-style trees; equal or nested roots are rejected in both the desktop UI and Rust backend. Re-approving a Folder Map invalidates earlier file approvals. File deletion and overwrite are not supported. Move/Undo activity is journaled locally, including execution-time move failures and retryable failed Undo attempts.

## Architecture
- React + TypeScript UI
- Tauri v2 desktop shell with Rust filesystem commands
- Local browser storage for Folder Map/classification preferences; local JSONL journal for Move/Undo audit
- SQLite remains a later persistence option, not a current runtime dependency
- GitHub main = source of truth
- Google Stitch export retained as UI reference

## Development
Routine changes run frontend/Rust CI and Web Preview Build. Desktop bundles use relative frontend asset paths; Web Preview uses its repository base path explicitly. Windows installer generation is intentionally manual-only while V0.4 stabilization continues.


## V0.4 Windows acceptance checkpoint

RC5 completed the disposable-file Windows runtime path on 2026-09-26: approved Folder Map routing, 2/2 safety validation, two-file Move, persisted journal history, two-file Undo, zero Move/Undo failures, and history restoration after app restart. Routine source hardening after this checkpoint is covered by CI; Windows installers remain explicit checkpoint builds only.
