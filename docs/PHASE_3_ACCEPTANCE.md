# Phase 3 automated acceptance (manual gate remains open)

**Status: IMPLEMENTED / AUTOMATED_TEST_PASS / UPSTREAM_MANUAL_GATE_PENDING. Not ACCEPTED.**

- Branch: `phase/fileorbit-2-transition`; baseline Phase 2 `14747e1`; Phase 3 implementation `ee745c6`.
- Full scan uses existing WalkDir safety guard and Phase 2 SQLite schema. Refresh walks metadata to detect removal, but unchanged rows avoid SQLite update. New and changed metadata upsert; unseen rows become `missing`. Moving a file between paths records the old path as `missing` and indexes the new path.
- Synthetic tests: first scan, repeat unchanged scan, new/modified/moved/deleted files, Korean names, excluded runtime directory, persistence after reopen. A further 1,200-file refresh and Unix symlink rejection test are in the current checkpoint pending CI.
- Automated checkpoint [36308475990](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36308475990): frontend, Linux/Windows Rust tests, Windows fixture, MSI and NSIS PASS. Later export adapter checkpoint [36353482719](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36353482719) pending Windows installer at document authoring time.
- Limitations: each refresh still enumerates the whole Test Root metadata tree; filesystem watcher and scan scheduling are future work. Historical move identity is path based. No actual Downloads, work roots, `C:\FileOrbit`, or `main` changes.
- Manual upstream gate: Phase 2 installed Windows app interactive Test Root E2E is **MANUAL_WINDOWS_E2E_PENDING**. Phase 3 cannot be marked ACCEPTED before that gate and its own installed-app review.
- Actual business files changed: **0**. Actual `C:\FileOrbit` changed: **0**.
