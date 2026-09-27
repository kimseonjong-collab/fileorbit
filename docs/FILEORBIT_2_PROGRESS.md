# FileOrbit 2.0 Progress

| Phase | Implementation | Automated validation | Manual validation | Gate |
| --- | --- | --- | --- | --- |
| 1 Standard paths | Complete | PASS | Disposable Windows Test Root still useful | Existing recovery baseline retained |
| 2 SQLite Index | Implemented | PASS, [CI 36307535465](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36307535465) | **MANUAL_WINDOWS_E2E_PENDING** | Do not mark ACCEPTED |
| 3 Full Scan + Incremental Refresh | Implemented, metadata traversal | PASS [CI 36308475990](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36308475990) | Upstream manual gate pending | IMPLEMENTED / AUTOMATED_TEST_PASS / UPSTREAM_MANUAL_GATE_PENDING |
| 4 Downloads Inbox | Synthetic index discovery implemented | PASS in same CI | Upstream manual gate pending | No automatic move; not ACCEPTED |
| 5 Candidate Engine | Deterministic SQLite-only search implemented | PASS in same CI | Upstream manual gate pending | Candidates only; not ACCEPTED |
| 6 Google Sheet Workspace | [FileOrbit_AI_Workspace](https://docs.google.com/spreadsheets/d/1HWdkfe0QO7Dtvppq4WFc4oAEyPapnov0Qn0v0W9Th7I/edit) created; Test Root export and preview model | Export PASS [CI 36353482719](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36353482719); preview CI PENDING | Upstream manual gate pending | SQLite remains source of truth; sync not connected |

## Current Phase 3 slice

- Existing WalkDir scanner safety and SQLite index are reused. Every refresh walks metadata to detect removals, while unchanged indexed rows avoid DB updates; only changed/new/missing entries alter indexed records.
- Synthetic fixture covers unchanged refresh, added/modified/moved/missing files, Korean paths and excluded runtime folders. This is **change-focused DB refresh**, not an OS filesystem event watcher.
- Existing scan/move/undo commands are unchanged. Actual business files changed: **0**; `C:\FileOrbit` changed: **0**; `main` changed: **0**.
- CI [36308190044](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36308190044): Linux Rust, frontend, SQLite, Windows fixture PASS; Windows Rust and MSI/NSIS checkpoint pending.
- Phase 4: schema v3 inbox review rows retain state across repeated discovery. Only disposable Test Root/testdata is accepted. Existing Downloads Review remains available; no file movement is invoked.
- Phase 5: candidates are read from present SQLite file/folder rows, with deterministic name/path token, extension, and date signals. This first slice has no prior decision/history signal and scans indexed rows in memory; it never rescans the filesystem or executes a proposal.
- Phase 6: native Google Sheet tabs `Inbox_Review`, `Candidates`, `Corrections`, `ReadMe` contain headers and no actual file metadata. Test Root only export rows use stable `inbox:<file_id>` identifiers and `not_synced`. A correction preview requires an explicit structured destination and validates synthetic source/target, without filesystem writes. Live sync, correction import and conflict handling remain PENDING implementation. Sheet edits never alter SQLite or files.
- CI [36308475990](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36308475990) passed frontend, Linux and Windows Rust tests, Windows fixture, MSI and NSIS. Phase 2 **MANUAL_WINDOWS_E2E_PENDING** remains the upstream manual gate; installed-app interactive test was not run by CI.
- Phase 6 export commit `d8945b1`; [CI 36353482719](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36353482719) passed frontend, Linux/Windows Rust, Windows fixture, MSI and NSIS. The additional bulk/symlink/preview checkpoint is pending.
- Actual business files changed: **0**; `C:\FileOrbit` changed: **0**; `main` changed: **0**.
- Next: export adapter CI and safe sync/correction data contract; then one disposable Windows installed-app interactive E2E with the user.
