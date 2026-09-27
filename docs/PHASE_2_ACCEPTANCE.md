# FileOrbit 2.0 Phase 2 Acceptance

- Branch baseline: `phase/fileorbit-2-transition@6c446cc164c777c5736e3dda91c5346a8fe899be`.
- Scope: Tauri SQLite index connection, v1/v2 migrations, synthetic Test Root metadata scan, query API and minimal UI status panel.
- DB path: the app currently uses its existing Tauri app data directory under `data/fileorbit.db`; disposable tests use a Test Root under the OS temporary directory or `C:\FileOrbit-Test` under `data/fileorbit.db`. Migration to `C:\FileOrbit` is separately gated and has **not** occurred.
- Schema version: 2. `user_version` and `schema_migrations` are updated transactionally; unsupported future versions and malformed DBs return errors without silent replacement.
- Synthetic Rust tests: fresh DB, repeat open, migration, Korean file/path, repeated scan, metadata update, new row, missing state, persistence after reopen, Test Root containment, locked/malformed DB. Linux PASS; Windows result pending current CI.
- Rescan: path keyed upsert; absent entries become `missing`; no physical file or row deletion. Existing move/undo commands are unchanged. The synthetic index is refreshed by rescan after external changes.
- Frontend build and disposable SQLite schema tests: PASS in local build and CI. Synthetic 3,000-row insert, update, search, path filter and reopen checks added for the final CI gate.
- Windows fixture: PASS on disposable synthetic paths. Windows installed-app interactive Test Root E2E: **PENDING**. CI backend tests and installer build do not establish interactive installed-app acceptance.
- Windows Rust / MSI / NSIS CI: PASS on [run 36306473546](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36306473546) for the backend implementation; final guard/performance commit CI remains PENDING.
- Actual business files changed: 0. Actual `C:\FileOrbit` changed: 0. `main` changed: 0.
- Known limits: no production-folder index enablement, no live move-to-index synchronization, no installed-app interactive verification, no full incremental scan optimization (Phase 3).
- Next gate: complete Windows CI, then run installed app against an isolated disposable Test Root before marking Phase 2 PASS or entering Phase 3.
