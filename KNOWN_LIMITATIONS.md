# Known Limitations — 0.5.0

- 7z installation uses the open-source 7-Zip command-line program available on
  the host. ZIP support is built in. A missing `7z` produces setup guidance.
- UE4SS can be installed from a package the user downloaded, but it is never
  downloaded automatically. The Nexus `nxm://` handoff can download a package
  after the user starts it on the website and configures a personal API key.
- UE4SS installation preserves `UE4SS-settings.ini`, `mods.txt`, `mods.json`,
  and every `load_order.txt`. A package shipping newer defaults for those will
  not replace an existing copy; remove yours first to adopt them.
- Applying a UE4SS start order writes the managed entries as one block after
  everything the manager does not own. Comments, blank lines, and the runtime's
  own entries keep their positions, but a managed mod that was hand-placed
  among them moves into that block. Order among managed mods is preserved, and
  mods installed before this release keep the order the file already has.
- UE4SS starts DLL mods and Lua mods in two separate passes: every DLL mod runs
  as the runtime initializes, and the Lua mods only once the scripting runtime
  exists. Order is therefore settable within each pass, never across them, and
  a request to interleave them is normalized into what the runtime will do.
- UE4SS start order covers `mods.txt` only. BPModLoader keeps its own list in
  `BPModLoaderMod/load_order.txt` for blueprint mods, which the manager still
  preserves rather than writes, so blueprint load order stays manual.
- An upgrade is reversible until it succeeds, not atomic afterwards. If the new
  version deploys but recording it fails, the new files are in place and the
  error says so; the old entry is the one left behind.
- Game-folder mods are recognized from three layouts: a tree containing
  `SWZeroCompany`, a `LogicMods` blueprint pack, and a loader shim named after
  the system library it replaces (`dxgi.dll`, `dinput8.dll`, and similar) with
  the files beside it. Anything else in the archive is listed and left alone.
- A game-folder mod is the only kind that replaces an existing file. The
  original is kept in the managed library and restored on disable or removal,
  but a file another mod already owns is never overwritten.
- Existing-mod discovery adopts additive PAK/IoStore, UE4SS, and LogicMods.
  It reports but does not adopt ReShade and other replacement-style game-folder
  mods because their pre-mod originals are no longer available to back up.
- Lua mods bundled inside a UE4SS package are treated as part of the runtime
  and are overwritten on upgrade. Edits to a shipped mod's scripts are lost;
  copy it under a new folder name to keep changes.
- Steam launch options are inspected heuristically and never edited. On Linux,
  confirm `WINEDLLOVERRIDES="dwmapi=n,b" %command%` manually.
- The optional custom game launcher starts the selected file with its containing
  folder as the working directory and does not add command-line arguments. On
  Linux, select a native launcher or wrapper rather than a Windows executable
  that the host cannot run directly.
- retoc can verify only containers supported by retoc 0.1.5. Encrypted or future
  game container formats may require an upstream update.
- PAK-only mods cannot provide package-level overlap metadata; only destination
  filename collision is available for them.
- Load-order management is enabled for IoStore triplets with a companion PAK,
  which passed the two-direction Zero Company runtime test. Pure UTOC/UCAS
  pairs remain visible but non-orderable because they are not independently
  verified.
- PAK-only mods remain visible but non-orderable: the local capability fixture
  did not pass the runtime gate. Their contents are also opaque, so the manager
  cannot identify which assets a PAK-only mod wins or loses.
- Hiding a mod affects the library list only. A hidden mod is still installed,
  still deployed, still counted on Home, and still listed in the load-order and
  UE4SS start-order editors, because it still loads and its position still
  matters.
- Removing or disabling a mod prunes the empty folders its own payload created,
  but only below that mod type's deployment base and only while they are empty.
  A folder still holding a settings file, a log, or anything else the manager
  does not own is kept, and a game-folder mod is never pruned because its base
  is the game installation itself.
- Full profiles remain future work.
- Update checking needs a mod to be matched to its Nexus page. A download
  through the `nxm://` handoff records that outright. Anything else is matched
  by offering the MD5 of the archive it was installed from, which only works
  while that archive is still on disk and only for a file that was actually
  uploaded to Nexus. A mod adopted from disk, installed from an archive that has
  since been deleted, or built from source is never matched automatically and
  has to be linked by hand from More details, or it is not checked.
- An update is the newest file Nexus still offers under `MAIN` or `UPDATE`, and
  newer means a higher file id, which Nexus issues in upload order. A mod that
  publishes its releases under another category is compared against its newest
  offered file instead; an author who re-uploads an old build under a new file
  id is reported as an update.
- The update check reports that a newer file exists. It never downloads or
  installs one on its own, and a free Nexus account has to start the download
  on the website because the API will not mint a download link without the
  website's key.
- Linking a mod by hand trusts the address given. The file recorded as installed
  is the one whose version string matches the installed version, and the newest
  offered file when no version matches, so a mod whose version was never
  recorded is linked to the current file and reports an update only from the
  next release onward.
- An update check is a request per Nexus mod, plus one per unmatched archive
  that has not been excluded, so a large library spends the hourly API allowance
  quickly. A mod that is not published on Nexus is worth taking out of checking
  from More details, because its archive is otherwise offered again on every
  check the user asks for. The result stands
  for six hours before an automatic check goes back to the network; the Mods
  page button always does.
- Downloads must be started from the Nexus Mods website. A non-premium account
  cannot obtain a download link from the API without the website-minted key, so
  no in-application browsing or search is offered.
- Without a Secret Service provider on Linux, the Nexus API key is stored in the
  application database as plain text. Settings reports which location is in use.
- The `nxm://` association is claimed only when enabled in Settings, so it is
  never taken from another mod manager silently. If another manager already
  holds it, Settings names that application instead of failing quietly.
- On Linux the `nxm://` desktop entry is written by this application rather
  than by `tauri-plugin-deep-link`, which quotes `Exec`. `xdg-mime` resolves an
  entry by passing the first whitespace-separated word of `Exec` to
  `command -v` without stripping quotes, so a quoted path never resolves and
  the entry is skipped silently. Paths that genuinely need quoting are reached
  through a symbolic link instead.
- `xdg-mime query` reads `<desktop>-mimeapps.list` before the generic
  `mimeapps.list` when `XDG_CURRENT_DESKTOP` is set, but `xdg-mime default`
  only writes the generic file. An application that claimed a scheme in the
  prefixed file keeps it regardless of later registrations, so the prefixed
  files are updated too — only where they already name the scheme, and the
  entry is removed again when the association is handed back.
- Release builds are unsigned. SmartScreen may warn on Windows.
- Flatpak/Snap sandbox permissions and uncommon portable Steam installations
  may require manual game-path selection.
- Windows artifacts are generated by GitHub Actions; they cannot be produced on
  a Linux host without a complete Windows MSVC/WiX/NSIS toolchain.
- AppImage builds on rolling distributions whose libraries use modern RELR ELF
  sections set `NO_STRIP=1`; the linuxdeploy binary embedded by Tauri otherwise
  uses an older `strip` that cannot parse those sections. Release CI also sets
  this compatibility flag.
