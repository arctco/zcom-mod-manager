# Changelog

All notable changes are documented here.

## 0.4.1

- Multi-component archives can carry a separate `zcom-mod.json` beside each
  component. The install review labels and checks each payload with its own
  metadata and offers one **Install all components** action for bundles that
  contain no mutually exclusive options.
- A bundle upgrade matches and replaces each previously installed component,
  including components that originally came from separate downloads. Failed
  installations keep their validated preview available for retry.

## 0.4.0

- Archives containing packaged variants in separate folders now present each
  folder as a labeled installation option. This fixes downloads such as
  Blackmarket Discounts, whose four strengths were previously flattened into
  one mod and would all be installed together.
- Added an optional custom game executable or launcher in Settings. Home uses
  it instead of the Steam URI when configured, and **Use Steam** clears the
  override.
- Added existing-mod discovery and adoption for PAK/IoStore containers, UE4SS
  folders, and additive LogicMods. A first-connection prompt and a permanent
  Mods-page action open a review wizard. Adoption copies payloads into the
  managed library without moving, renaming, or rewriting live files, and each
  selected group succeeds or fails independently.
- Migration preserves UE4SS enabled state and order from `mods.txt`, supports
  merging packaged container families, excludes already managed destinations,
  blocks incomplete or unverifiable IoStore sets, and reports replacement-style
  injectors that cannot be adopted without an original-file backup.

## 0.3.0

- Added UE4SS start-order management. The Load order tab lists UE4SS mods in
  start order and writes the managed block of `mods.txt` back. The runtime uses
  two passes — every DLL mod starts as UE4SS initializes, the Lua mods only once
  the scripting runtime exists — so the editor sets order within each pass and
  normalizes any request to interleave them. Comments, blank lines, and the
  runtime's own entries keep their position, and mods installed before this
  release keep the order the file already has.
- Added in-place upgrades. A newer build of an installed mod is recognized at
  inspection and offered as a replacement instead of a deployment conflict. The
  previous version's files are moved aside rather than deleted, so a failure
  anywhere in the new installation puts them back and leaves the old version
  installed. The replacement keeps its predecessor's position in the load order,
  and inherits the original files a game-folder mod displaced.
- Every UE4SS mod folder in an archive now installs as its own library entry,
  so mods that shipped together can be enabled, ordered, and removed separately.
- UE4SS DLL mods install. A mod folder is recognized by `Scripts/main.lua` or
  `dlls/*.dll`, so native mods such as Unique Talents for All and ZCUnlocked
  are no longer rejected as unrecognized payloads.
- Every UE4SS mod folder inside one archive is installed, instead of only the
  first, each with its own `mods.txt` line.
- Archives written on Windows with `\` separators extract correctly on Linux.
  Their entries previously became single files with backslashes in the name, so
  the mod layout never appeared and detection failed.
- Mods are named after the download rather than after the first file inside it.
  Nexus publishing metadata (mod id, version, upload stamp, and the random
  suffix) is stripped, and the version it carries is kept.
- Mod names can be edited before installing and renamed afterwards from the
  library. Renaming changes the label only; deployed files keep their names.
- Added game-folder mods: ReShade and other loader shims, replacement movies
  and audio, and `LogicMods` blueprint packs. A file the mod replaces is kept
  in the managed library and restored when the mod is disabled or removed.
- A UE4SS runtime package dropped on the installer is now recognized as the
  runtime and offered as a runtime install, instead of being taken apart into
  the mods it ships.
- An archive holding several mods is previewed as several mods, each named and
  installed on its own.
- Files an archive contains that are not part of a recognized layout are listed
  before installing, and native code inside a mod is called out as such rather
  than reported as ignored.
- The UE4SS mod count on Home and in diagnostics counts DLL mods too.
- The load-order tab now says where the mods it does not list are ordered
  instead, so a library of UE4SS mods no longer looks like the editor lost them.
- Fixed doubled event subscriptions. The unsubscribe handle arrives after the
  effect is cleaned up, so every listener was registered twice: one dropped
  archive was inspected twice and one `nxm://` link would download twice. A
  superseded inspection now also releases its own extraction sandbox, and stale
  sandboxes are cleared at startup.
- Library row actions are laid out as two rows of three; six buttons never fit
  the single row they were given.
- An upgrade of a mod installed on a different drive from the managed library
  no longer fails before it starts. Moving the previous version's files aside
  used a rename, which cannot cross a filesystem boundary, and the game is
  regularly on a second drive.
- Payload paths shown in the install preview use one separator on every
  platform instead of mixing both on Windows.

## 0.2.0

- Fixed Home and Settings folder actions by moving trusted path opening behind
  validated native commands and surfacing failures in the interface.
- Added a Home-page game launch button that opens Zero Company through Steam,
  preserving the user’s Steam and Proton launch configuration.
- GitHub release checks now run once at startup. When a newer release exists,
  a compact update icon appears beside About; the manual retry remains there.
- Added a conflict-aware load-order editor for runtime-verified IoStore
  triplets. PAK-only mods and pure IoStore pairs remain visible but gated.
- Added deterministic numeric `_P` deployment ranks, with the highest row
  winning known package overlaps.
- Added active and potential conflict states plus winner previews that keep raw
  package paths private.
- Added review-before-apply, SHA-256 preflight checks, rollback-safe renames,
  and startup recovery for interrupted load-order operations.
- Newly installed runtime-supported packaged mods default to the highest
  priority and normalize the existing managed order in the same confirmed
  installation flow.
- Archives containing overlapping IoStore container variants are now rejected
  with guidance to install only one variant.
- Added a real SQLite v2 migration while preserving existing mod ownership and
  enabled states.

## 0.1.5

- Restored the original 0.1.0 blue, steel, and gold interface palette while
  preserving the newer application layout and features.
- Added an About page with the installed version, project information, license,
  source links, and release links.
- Added an on-demand GitHub update check that compares the installed version
  with the latest published release and links directly to available updates.
- Corrected repository, release API, documentation, and external-link targets
  to `arctco/zcom-mod-manager`.

## 0.1.4

- Added a portable Windows zip to the release artifacts. Extract it anywhere and
  run `ZCOM Mod Manager.exe`. The bundled `retoc.exe` ships beside it and is
  detected automatically, so IoStore verification works without configuration.
  The portable build expects the Microsoft Edge WebView2 runtime to already be
  installed; the NSIS installer still fetches it when missing.
- Fixed the release step that packages that zip. It looked for the executable
  under the product name, but `tauri build` leaves the cargo package name in
  `target/release`, so 0.1.3 published without the zip its notes announced.

## 0.1.3

- Removed the MSI bundle. NSIS is now the only Windows installer.

## 0.1.2

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

- Restored the original Z application icon, drawn as plain SVG in
  `src-tauri/icons/app-icon.svg`. The clone trooper helmet artwork used for the
  0.1.1 icon was withdrawn at the artist's request and is no longer distributed.

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
