# Changelog

All notable changes are documented here.

## Unreleased

- Added the Nexus Mods `nxm://` handoff. **Mod Manager Download** on the website
  hands the link to the manager, which fetches the file and routes it through
  the existing review and validation path.
- Added Nexus API key storage in the OS secret store, with a plain-text database
  fallback that the interface discloses.
- Added an opt-in `nxm://` protocol registration toggle; the association is
  never claimed automatically.
- Links for other games are refused rather than downloaded.
- Settings names the application currently holding `nxm://` when registration
  does not take effect.
- Registered the Linux `nxm://` handler directly instead of through
  `tauri-plugin-deep-link`, whose quoted `Exec` line `xdg-mime` can never
  resolve, and claimed the scheme in `<desktop>-mimeapps.list`, which
  `xdg-mime query` reads before the file `xdg-mime default` writes.

- Fixed UE4SS upgrades leaving runtime-supplied Lua mods at their old version.
  Only `UE4SS-settings.ini`, `mods.txt`, `mods.json`, and `load_order.txt` are
  preserved now; mods a package ships move with the runtime, and mods the user
  installed are untouched because a package never contains them.
- Added an opt-in test that runs the installer against a real published UE4SS
  package via `ZCOM_UE4SS_ARCHIVE`.

## 0.1.1

- Added a guided UE4SS runtime installer that unpacks a user-downloaded package
  into `Binaries/Win64` while preserving `ue4ss/Mods/` and `UE4SS-settings.ini`.
- Added links to the tested Zero Company UE4SS build and to the game's Nexus
  Mods page, scoped through the opener capability allowlist.
- Added search and status filtering to the Mods page.
- Reworked the application icon and interface palette around the clone trooper
  helmet artwork.

## 0.1.0

- Added cross-platform Steam library and build discovery with manual fallback.
- Added ZIP, 7z, direct packaged-file, folder, and drag-and-drop intake.
- Added IoStore companion validation and retoc 0.1.5 verification.
- Added PAK-only and existing-runtime UE4SS Lua mod management.
- Added an SQLite managed library with SHA-256 ownership records.
- Added transactional deployment, safe enable/disable, verify, and uninstall.
- Added filename/package overlap detection with spoiler-safe defaults.
- Added manifest compatibility warnings and game-update detection.
- Added Mod Doctor, UE4SS layout checks, and Linux/Proton guidance.
- Added Linux and Windows CI/release workflows, documentation, and licensing.
