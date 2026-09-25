# FileOrbit Move & Undo Safety Contract

## Status
V0.3 keeps all file mutation locked. This contract defines the conditions required before enabling Move in the next checkpoint.

## Preconditions
1. A Folder Map must be explicitly approved and persisted.
2. Downloads must be explicitly selected and scanned read-only.
3. Every file to move must have an explicit user decision of `approved`.
4. Files marked `hold` or without a confirmed target are never moved.
5. The target must remain inside the approved Folder Map root.
6. Source and target paths are normalized before execution.
7. Existing target files are never overwritten silently.

## Transaction journal
Before each move, write a journal entry containing: transaction id, timestamp, source, target, original size, status=`planned`.
After success, update status to `moved`. On failure, update status to `failed` with the error.
Undo operates only on `moved` entries and restores the original source when it is safe to do so.

## Collision policy
If the target exists, stop that item and mark it `collision`. Do not auto-rename in the first executable Move version.

## Failure policy
Moves are item-by-item, not all-or-nothing. A failed item does not cause silent retries. The review screen must show moved, failed, collision, and held counts.

## Safety invariant
No background moves. No scheduled moves. No deletion. No overwrite. No move without explicit approval. Undo must be available before executable Move is enabled.
