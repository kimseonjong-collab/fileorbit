# FileOrbit 2.0 Progress

| Phase | Implementation | Automated validation | Manual validation | Gate |
| --- | --- | --- | --- | --- |
| 1 Standard paths | Complete | PASS | Disposable Windows Test Root still useful | Existing recovery baseline retained |
| 2 SQLite Index | Implemented | PASS, [CI 36307535465](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36307535465) | **MANUAL_WINDOWS_E2E_PENDING** | Do not mark ACCEPTED |
| 3 Full Scan + Incremental Refresh | Implemented, metadata traversal | Linux PASS, Windows checkpoint PENDING | Upstream manual gate pending | Do not mark ACCEPTED |
| 4 Downloads Inbox | Synthetic index discovery implemented | PENDING CI | Upstream manual gate pending | No automatic move |
| 5 Candidate Engine | Deterministic SQLite-only search implemented | PENDING CI | Upstream manual gate pending | Candidates only |
| 6 Google Sheet Workspace | Not started | PENDING | Authentication/connection to check | SQLite remains source of truth |

## Current Phase 3 slice

- Existing WalkDir scanner safety and SQLite index are reused. Every refresh walks metadata to detect removals, while unchanged indexed rows avoid DB updates; only changed/new/missing entries alter indexed records.
- Synthetic fixture covers unchanged refresh, added/modified/moved/missing files, Korean paths and excluded runtime folders. This is **change-focused DB refresh**, not an OS filesystem event watcher.
- Existing scan/move/undo commands are unchanged. Actual business files changed: **0**; `C:\FileOrbit` changed: **0**; `main` changed: **0**.
- CI [36308190044](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36308190044): Linux Rust, frontend, SQLite, Windows fixture PASS; Windows Rust and MSI/NSIS checkpoint pending.
- Phase 4: schema v3 inbox review rows retain state across repeated discovery. Only disposable Test Root/testdata is accepted. Existing Downloads Review remains available; no file movement is invoked.
- Phase 5: candidates are read from present SQLite file/folder rows, with deterministic name/path token, extension, and date signals. This first slice has no prior decision/history signal and scans indexed rows in memory; it never rescans the filesystem or executes a proposal.
- Next: Windows checkpoint CI and regression; manual installed-app E2E remains outstanding.
