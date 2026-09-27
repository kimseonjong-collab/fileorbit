# FileOrbit 2.0 Transition Diagnosis

## Phase 0 — Baseline diagnosis

Date: 2026-09-27  
Repository: kimseonjong-collab/fileorbit  
Recovery baseline: `main@ba3b3d5360b449b8882879fc8045d5efa7b54440`

## 1. Phase 0 conclusion

The current FileOrbit application is a React + Tauri desktop application. The existing baseline contains reusable assets for folder scanning, Downloads review, safe file moves, and Undo. FileOrbit 2.0 should therefore evolve the working baseline instead of being rewritten from scratch.

The Phase 0 objective is complete: establish a recoverable baseline, identify reusable assets, separate keep/strengthen/new work, and define safe acceptance gates before any real user files are migrated.

## 2. Baseline protection

- Keep `main@ba3b3d5360b449b8882879fc8045d5efa7b54440` as the recovery point.
- Do not migrate real files to `C:\\FileOrbit` until dry-run and test-data gates pass.
- Do not delete or bulk-move user business files during transition development.
- Every destructive-capable operation must have preview/dry-run semantics.
- Batch operations must retain enough metadata to support Undo.

## 3. Asset decision

### Keep and reuse

- React UI foundation.
- Tauri desktop/runtime foundation.
- Existing folder scan capability.
- Existing Downloads review workflow.
- Existing safe-move implementation and related safety rules.
- Existing Undo assets and acceptance evidence.
- Existing release/CI baseline unless a 2.0 requirement requires extension.

### Strengthen

- Standardized application/data paths.
- File index persistence and schema migration.
- Incremental refresh after initial full scan.
- Move safety: collision handling, destination validation, audit metadata.
- Batch-level Undo.
- Test-data fixtures and deterministic dry-run acceptance.
- Recovery documentation.

### New for 2.0

- Standard `C:\\FileOrbit` directory structure.
- Local SQLite Index as FileOrbit's durable local memory.
- Initial full scan followed by change-focused refresh.
- Downloads Inbox backed by the local Index.
- Related file / related folder candidate extraction from the Index.
- Google Sheet integration for `FileOrbit_AI_Workspace`.
- Five-stage Cockpit UI.
- Safe folder creation and Quarantine workflow.
- Data flow that can receive ChatGPT analysis results and user natural-language corrections, convert them into proposed actions, and execute only after safety validation.

## 4. Target standard directory structure

The exact physical move is NOT performed in Phase 0. The target structure is:

```text
C:\FileOrbit\
  app\
  data\
    fileorbit.db
    backups\
  config\
  logs\
  workspace\
  quarantine\
  testdata\
```

Rules:

- `app`: application/runtime assets only.
- `data`: SQLite and data backups.
- `config`: local configuration and path registry.
- `logs`: operational/audit logs.
- `workspace`: controlled intermediate files, never the user's arbitrary business folders.
- `quarantine`: reversible isolation target; no permanent deletion semantics.
- `testdata`: deterministic fixtures used before real-file validation.

## 5. Transition phases and acceptance gates

### Phase 1 — Standard paths

Implement path registry and directory bootstrap without moving existing user files.

Pass when:

- Dry-run lists all directories that would be created.
- Test mode creates the structure only under an isolated test root.
- Existing app can still start against the old baseline.
- No business file is moved.

### Phase 2 — SQLite Index

Implement local SQLite schema and migrations.

Minimum records:

- files
- folders
- scan roots
- scan runs
- change state
- proposed actions
- executed actions
- batches
- undo metadata

Pass when:

- Database can be created from zero.
- Migration is repeatable/idempotent.
- Test scan can write and read indexed records.
- Corrupt/failed transaction does not leave partial committed action state.

### Phase 3 — Full scan + incremental refresh

Implement first full scan and subsequent change-focused refresh.

Pass when:

- First scan indexes deterministic fixtures.
- Second unchanged scan does not rewrite everything unnecessarily.
- Add/modify/move/delete fixture cases are detected correctly.
- Ignored/runtime folders are excluded by rule.

### Phase 4 — Downloads Inbox

Use the Index to present Downloads review candidates.

Pass when:

- Inbox reads from indexed state.
- Candidate refresh does not require full rescan when unchanged.
- No move occurs before explicit execution.
- Proposed destination and rationale are visible.

### Phase 5 — Related candidates

Extract related files and related folders using existing indexed metadata.

Pass when:

- Candidate generation is deterministic for fixtures.
- A confidence/rationale payload exists.
- Candidate generation itself never moves files.

### Phase 6 — FileOrbit_AI_Workspace Sheet

Add Google Sheet synchronization as an integration layer, not as the local source of truth.

Pass when:

- SQLite remains authoritative for local filesystem state.
- Sheet read/write failure does not corrupt local Index.
- Sync state and errors are visible/auditable.
- Duplicate processing is prevented with stable identifiers.

### Phase 7 — Five-stage Cockpit

Recommended stages:

1. Observe
2. Review
3. Decide
4. Execute
5. Verify / Undo

Pass when:

- User can distinguish proposal from execution.
- Every executable action exposes dry-run/preview.
- Execution results are linked to a batch and audit record.

### Phase 8 — Safe actions

Implement/strengthen:

- Move
- Folder Create
- Quarantine

Pass when:

- Source existence and destination validity are checked.
- Name collisions are handled without silent overwrite.
- Partial batch failure is recorded item by item.
- Quarantine is reversible and is not permanent delete.

### Phase 9 — Batch Undo

Pass when:

- Every action batch has a stable batch ID.
- Undo validates current filesystem state before reversal.
- Conflicts do not overwrite existing files.
- Partial Undo produces a clear residual-state report.

### Phase 10 — ChatGPT/user correction data flow

Use structured proposals, not unrestricted natural-language execution.

Suggested lifecycle:

`analysis -> proposal -> user correction -> normalized action plan -> safety validation -> dry-run -> execute -> verify -> undo metadata`

Pass when:

- Natural-language input never directly invokes raw filesystem commands.
- Every normalized action is validated against allowed action types.
- Proposed and executed payloads are retained.
- User corrections can change destination/classification before execution.

## 6. Safety policy for transition work

Until all relevant gates pass:

- Use test fixtures first.
- Use dry-run for action generation.
- Do not perform bulk operations on the user's two primary work folders.
- Do not permanently delete.
- Do not silently overwrite.
- Do not treat Google Sheet as the filesystem source of truth.
- Preserve the baseline recovery commit and existing release assets.

## 7. Phase 0 evidence

Confirmed on GitHub on 2026-09-27:

- Repository exists and is accessible.
- Default branch: `main`.
- Connected account has admin/push permission.
- Current latest `main` commit is `ba3b3d5360b449b8882879fc8045d5efa7b54440`.
- Existing `docs/` contains architecture, move/undo safety, and acceptance documents that should be reused.

## 8. Current limitations

Phase 0 does not prove Windows runtime behavior on the user's PC.

Not yet completed:

- Physical migration to `C:\\FileOrbit`.
- SQLite implementation.
- Full/incremental scan implementation for 2.0.
- FileOrbit_AI_Workspace integration.
- Five-stage Cockpit implementation.
- Windows Rust/Tauri runtime acceptance for new 2.0 code.
- Real-file migration.

## 9. Next implementation step

Proceed with Phase 1 on an isolated transition branch:

1. introduce a standard path registry,
2. add test-root bootstrap,
3. add dry-run output for directory creation,
4. verify no existing real file is moved,
5. then proceed to SQLite schema work.

No user business-file action is required at this stage.
