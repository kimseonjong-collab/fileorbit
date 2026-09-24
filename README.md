# FileOrbit

Local-first folder analysis and download organization tool.

## V0.1 scope
- Existing work-folder analysis first
- Read-only Folder Doctor
- Folder Map / Folder Profile
- Dry Run before any change
- No background resident process
- No scheduled execution
- No automatic move without explicit user action
- Downloads classification comes after the destination Folder Map is established

## Safety baseline
V0.1 does not move or delete user files.

## Architecture
- React + TypeScript UI
- Tauri v2 desktop shell
- Local SQLite for profiles/history only
- GitHub main = source of truth
- Google Stitch export retained as UI reference
