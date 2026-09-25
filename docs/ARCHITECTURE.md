# FileOrbit Architecture

## Product workflow
1. User explicitly selects an existing work-folder root.
2. Folder Doctor performs a read-only scan and excludes development/cache/temp noise.
3. FileOrbit builds Folder Profiles from structure and metadata.
4. Dry Run and role-based recommendations are reviewed without disk mutation.
5. User explicitly approves the destination structure as a persistent Folder Map.
6. Downloads Organizer scans a user-selected folder only on demand.
7. Classification uses Folder Map keywords plus local correction feedback.
8. User reviews each candidate and explicitly approves or holds it.
9. Backend validates source root, approved destination root, traversal/collision conditions, and the final Move gate.
10. Approved files are moved with a local transaction journal; history/audit and Undo remain available after restart.

## V0.4 safety boundary
- No background or scheduled file move.
- No automatic delete.
- No silent overwrite.
- No move without an approved Folder Map and explicit per-file approval.
- Destination path validation is enforced again in Rust, not trusted to the UI. Existing target-parent paths are canonicalized so a symlink/junction-style escape outside the approved Folder Map is rejected.
- Cross-volume fallback uses temporary copy, byte-size verification, final rename, then source removal; the fallback path is regression-tested independently of platform rename behavior.
- If source removal fails during cross-volume fallback, FileOrbit attempts to roll back the created destination.
- Undo is journal-driven and stops on collisions or missing moved files; the collision precheck is regression-tested so the original and moved files remain untouched.
- Execution-time Move failures are journaled as `failed` after a `planned` entry, preserving the audit trail when the filesystem changes after validation.
- Failed Undo attempts are recorded separately and do not destroy later retry eligibility.

## Local persistence
Current runtime persistence is local only:
- Folder Map: browser local storage.
- Download review/override state: scoped to Folder Map approval version (`approvedAt`) + Folder Map root + selected Downloads root; re-approving the map invalidates stale per-file approval state.
- Classification feedback: scoped to the approved Folder Map.
- Move/Undo audit: JSONL journal in the Tauri app-data directory; appended records are explicitly flushed to disk before success is returned.
- File bodies are not uploaded.

SQLite remains a future persistence option and is not required by V0.4.

## Runtime surfaces
- Web Preview: demo data only; cannot access or move PC files.
- Windows desktop: real read-only scans and explicit Safe Move/Undo.
- GitHub `main` is the source of truth.
