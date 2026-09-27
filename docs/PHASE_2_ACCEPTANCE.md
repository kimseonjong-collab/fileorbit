# FileOrbit 2.0 Phase 2 Acceptance

**Status: MANUAL_WINDOWS_E2E_PENDING — automated gates PASS. Later phase implementation may proceed; no phase is ACCEPTED until this gate passes.**

- Branch baseline: `phase/fileorbit-2-transition@6c446cc164c777c5736e3dda91c5346a8fe899be`.
- Scope: Tauri SQLite index connection, v1/v2 migrations, synthetic Test Root metadata scan, query API and minimal UI status panel.
- DB path: the app currently uses its existing Tauri app data directory under `data/fileorbit.db`; disposable tests use a Test Root under the OS temporary directory or `C:\FileOrbit-Test` under `data/fileorbit.db`. Migration to `C:\FileOrbit` is separately gated and has **not** occurred.
- Schema version: 2. `user_version` and `schema_migrations` are updated transactionally; unsupported future versions and malformed DBs return errors without silent replacement.
- Synthetic Rust tests: fresh DB, repeat open, migration, Korean file/path, repeated scan, metadata update, new row, missing state, persistence after reopen, Test Root containment, locked/malformed DB. Linux and Windows PASS.
- Rescan: path keyed upsert; absent entries become `missing`; no physical file or row deletion. Existing move/undo commands are unchanged. The synthetic index is refreshed by rescan after external changes.
- Frontend build and disposable SQLite schema tests: PASS in local build and CI. Synthetic 3,000-row insert, update, search, path filter and reopen checks PASS.
- Windows fixture: PASS on disposable synthetic paths. `scripts/phase2-windows-test-root.ps1` prepares and safely cleans a uniquely named TEMP fixture; its CI lifecycle test PASS. Windows installed-app interactive Test Root E2E: **PENDING**. CI backend tests and installer build do not establish interactive installed-app acceptance.
- Linux Rust, Windows Rust, Windows fixture, MSI and NSIS CI: PASS on [run 36307535465](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36307535465) for source `f669d58`. Installer artifact: [CI artifact 10927977385](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36307535465/artifacts/10927977385).
- Actual business files changed: 0. Actual `C:\FileOrbit` changed: 0. `main` changed: 0.
- Known limits: no production-folder index enablement, no live move-to-index synchronization, no installed-app interactive verification, no full incremental scan optimization (Phase 3).
- Next gate: complete Windows CI, then run installed app against an isolated disposable Test Root before marking Phase 2 PASS or entering Phase 3.

## One-time Windows installed-app acceptance

1. Use the MSI/NSIS artifact from the final passing CI run in a disposable Windows test session. Keep the existing stable FileOrbit installation available for rollback.
2. Run `./scripts/phase2-windows-test-root.ps1 -Mode Prepare` in PowerShell and copy its `TestRoot` and `ScanRoot` output into the app's **로컬 Index · Test Root** panel.
3. Scan and confirm four indexed files. Exit the app completely, relaunch it, enter the same `TestRoot` and select **저장된 Index 조회** before rescanning; confirm four saved rows. Then rescan and confirm no duplicate rows.
4. Add one synthetic file under `ScanRoot`, scan again and confirm five. Edit one file and confirm its size changes. Remove one synthetic file and confirm it is shown as `missing` while present count drops.
5. Run `./scripts/phase2-windows-test-root.ps1 -Mode Verify -Root '<TestRoot>'`. Record screenshots or a short result log. After accepting, close the app and run `-Mode Cleanup -Root '<TestRoot>'`.
