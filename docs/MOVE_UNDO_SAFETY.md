# FileOrbit Move & Undo Safety Contract

## Status
V0.4 enables explicit Safe Move/Undo on the Windows desktop only after the user approves a Folder Map, approves individual files, performs the final Move confirmation, and the Rust backend passes its safety checks. Web Preview remains demo-only and cannot move PC files.

## Preconditions
1. A Folder Map must be explicitly approved and persisted.
2. Downloads must be explicitly selected and scanned read-only.
3. Every file to move must have an explicit user decision of `approved`.
4. Files marked `hold` or without a confirmed target are never moved.
5. The source must remain inside the selected Downloads root.
6. The target must remain inside the approved Folder Map root.
7. Parent-directory traversal and unsafe relative path elements are rejected.
8. Existing target files are rejected; FileOrbit never silently overwrites them.
9. The backend re-validates the plan immediately before execution.

## Transaction journal
Each attempted move is recorded in the local app-data JSONL journal. A `planned` entry is written before mutation, followed by `moved` or `failed`. Undo writes `undone` or `undo_failed`. History is reloaded from this journal after restart.

## Move behavior
Same-volume moves use the filesystem rename primitive. If rename cannot complete, the cross-volume fallback copies to a temporary file in the prepared destination directory, verifies byte size, renames the temporary file to the final destination, and only then removes the source. If source removal fails, FileOrbit attempts to remove the newly created destination and reports the move as failed.

## Undo behavior
Undo is journal-driven. It refuses to overwrite an existing original path and refuses to proceed if the moved target is missing. An `undo_failed` entry records the failure without consuming the prior successful `moved` state, so the transaction remains retryable after the external cause is resolved.

## Failure policy
Moves are item-by-item, not all-or-nothing. A failed item is reported and journaled. There are no background or scheduled retries.

## Safety invariant
No background moves. No scheduled moves. No automatic deletion. No silent overwrite. No move without explicit approval and backend validation. Windows installer generation remains manual-only during V0.4 stabilization.
