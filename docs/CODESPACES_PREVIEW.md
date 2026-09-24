# FileOrbit — install-free browser preview

The repository remains private.

## First use
1. Open the repository in GitHub.
2. Select **Code → Codespaces → Create codespace on main**.
3. Wait for the browser editor to open.
4. Run the VS Code task **FileOrbit: Web Preview**.
5. Open forwarded port **4173**.

The preview uses the built-in demo scan dataset. It does not read, move, delete, or rename files on the Windows PC.

## What this validates
- React UI and layout
- Folder Profile rendering
- Folder Doctor flow
- Dry Run proposal logic
- Future Folder Map UI/logic

## What still requires the Windows app
- Real C:\ folder selection/scanning
- Windows filesystem permissions
- Fasoo/DRM behavior
- Installer, updater, and Authenticode behavior

Use Windows installer builds only at release checkpoints rather than on every commit.
