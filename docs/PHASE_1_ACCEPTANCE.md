# FileOrbit 2.0 Phase 1 acceptance

- Baseline: `main@ba3b3d5360b449b8882879fc8045d5efa7b54440`
- Branch: `phase/fileorbit-2-transition`
- Implementation: isolated `bootstrap_paths(test_root, dry_run)` command; existing scan, move, and undo commands remain in place.
- Preview: returns the planned standard directories without creating them. The DB file is a registry path only and is not created in Phase 1.
- Test mode: only an explicit absolute Test Root is accepted. Existing symlinks at the root or directory slots are rejected. The actual `C:\FileOrbit` is excluded.
- Local frontend build: PASS (`npm run build`).
- Rust unit tests / Linux and Windows CI: PENDING. Rust is unavailable in this workspace and Git push authentication failed. Do not mark Phase 1 accepted until CI passes.
- Windows installed-app acceptance: PENDING. Do not bootstrap the actual `C:\FileOrbit` yet.
- Actual business files changed: 0.
- Next gate: publish the branch through an authenticated GitHub connection, run CI including `cargo test`, and resolve failures before starting Phase 2.
