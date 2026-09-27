# FileOrbit 2.0 Phase 1 acceptance

- Baseline: `main@ba3b3d5360b449b8882879fc8045d5efa7b54440`
- Branch: `phase/fileorbit-2-transition`
- Implementation: isolated `bootstrap_paths(test_root, dry_run)` command; existing scan, move, and undo commands remain in place.
- Preview: returns the planned standard directories without creating them. The DB file is a registry path only and is not created in Phase 1.
- Test mode: only an explicit absolute Test Root is accepted. Existing symlinks at the root or directory slots are rejected. The actual `C:\FileOrbit` is excluded.
- Local frontend build: PASS (`npm run build`).
- Automated acceptance: PASS. [CI run 36304943223](https://github.com/kimseonjong-collab/fileorbit/actions/runs/36304943223) completed successfully on source `d26c11a`: frontend build, Linux `cargo check` and `cargo test`, Windows fixture, Windows `cargo test`, Windows MSI/NSIS build and artifact upload.
- Regression: PASS for existing scan, move and undo tests in Linux and Windows CI. The new bootstrap command is separate from those paths.
- Windows installed-app manual acceptance: `WINDOWS_ACCEPTANCE`, pending on a disposable Test Root. Do not bootstrap the actual `C:\FileOrbit` yet.
- Actual business files changed: 0.
- Phase 1 automated gate: PASS. Phase 2 may begin against the Test Root model. Actual PC path migration remains blocked pending separate Windows acceptance.
