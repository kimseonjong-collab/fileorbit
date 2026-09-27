# FileOrbit 2.0 Progress

| Phase | Implementation | Automated validation | Manual validation | Gate |
| --- | --- | --- | --- | --- |
| 1 Standard paths | Complete | PASS | Disposable Windows Test Root still useful | Existing recovery baseline retained |
| 2 SQLite Index | Implemented | PASS, [CI 36307535465](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36307535465) | **MANUAL_WINDOWS_E2E_PENDING** | Do not mark ACCEPTED |
| 3 Full Scan + Incremental Refresh | In progress | PENDING current CI | Upstream manual gate pending | Do not mark ACCEPTED |
| 4 Downloads Inbox | Not started | PENDING | Upstream manual gate pending | Synthetic only |
| 5 Candidate Engine | Not started | PENDING | Upstream manual gate pending | Proposal only |
| 6 Google Sheet Workspace | Not started | PENDING | Authentication/connection to check | SQLite remains source of truth |

## Current Phase 3 slice

- Existing WalkDir scanner safety and SQLite index are reused. Every refresh walks metadata to detect removals, while unchanged indexed rows avoid DB updates; only changed/new/missing entries alter indexed records.
- Synthetic fixture covers unchanged refresh, added/modified/moved/missing files, Korean paths and excluded runtime folders. This is **change-focused DB refresh**, not an OS filesystem event watcher.
- Existing scan/move/undo commands are unchanged. Actual business files changed: **0**; `C:\FileOrbit` changed: **0**; `main` changed: **0**.
- Next: CI and regression, then Phase 4 synthetic Downloads Inbox; manual installed-app E2E remains outstanding.
