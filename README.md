<div align="center">
  <img src="images/icon.png" alt="" width="160" height="160">
  <h1>ZCOM Mod Manager</h1>
</div>

A dedicated open-source mod manager for **Star Wars: Zero Company**.

ZCOM Mod Manager understands Zero Company mod payloads instead of
treating them as arbitrary files. It discovers Steam installations, validates
IoStore containers with retoc, manages UE4SS script and DLL mods, installs
game-folder mods such as ReShade, detects package overlap,
records SHA-256 ownership, and treats Linux/Proton as a first-class platform.

> This is an independent community project. It is not affiliated with or
> endorsed by Electronic Arts, Lucasfilm, Disney, Bit Reactor, or Nexus Mods.
> Star Wars and related names are trademarks of their respective owners.

## Features

- Steam AppID `2075800` discovery across default and additional libraries
- Steam-aware game launch from the Home page, with an optional custom executable
- Dynamic Steam build-ID detection and update warnings
- ZIP, 7z, direct PAK/UTOC/UCAS, folder, picker, and drag-and-drop input
- Nested archive payload discovery without copying documentation or random data
- Separate, labeled choices for packaged variants bundled in sibling folders
- IoStore pair/triplet validation and retoc 0.1.5 verification
- PAK-only and UE4SS Lua/DLL mod support
- Existing-mod discovery and non-destructive adoption for packaged, UE4SS, and
  additive `LogicMods` installations
- Manager-owned source library plus checksum-guarded deployment records
- Enable, disable, hide, verify, and safe uninstall operations
- Filename and hashed IoStore package conflict detection
- Conflict-aware packaged-mod load order with preview and rollback
- Optional spoiler-sensitive package paths, disabled by default
- UE4SS layout checks and formatting-preserving `mods.txt` updates
- Guided UE4SS runtime installation from a package you downloaded yourself
- Opt-in Nexus Mods `nxm://` download handoff with protected API-key storage
- On-demand update checking, with MD5 identification for mods installed before
  the manager tracked provenance and an opt-in throttled check at start-up
- Linux compatdata and Proton DLL-override diagnostics
- Sanitized structured logs and a copyable Mod Doctor report
- Automatic release notices with a manual retry on the About page, offering the
  GitHub release and the manager's own Nexus Mods page alike
- No account or always-on network requirement, telemetry, analytics, or advertisements

## Screenshots

The application ships a restrained tactical dark interface with six primary
areas: Home, Mods, Install, Diagnostics, Settings, and About. Release
screenshots are kept in `docs/screenshots/` when captured from a tagged build.

The interface contains no extracted game assets. The application icon is the
project's own Z mark, drawn as plain SVG in `src-tauri/icons/app-icon.svg`.

## Supported Platforms

| Platform | Architecture | Packages |
| --- | --- | --- |
| Linux | x86_64 | AppImage and `.deb` |
| Windows 10/11 | x86_64 | NSIS `.exe` installer and portable `.zip` |

Steam Deck/SteamOS should work through the x86_64 Linux AppImage. Add it as a
non-Steam application if desired. See [Known Limitations](KNOWN_LIMITATIONS.md)
for the boundaries of v0.4.0.

## Supported Mod Types

### IoStore packaged mods

The common payload is one logical container:

```text
Example_P.pak
Example_P.utoc
Example_P.ucas
```

UTOC and UCAS must share a basename and both must be present. A companion PAK
is installed when supplied. retoc must verify every UTOC before installation.

When an archive contains packaged alternatives in separate folders, the install
review presents each folder as a labeled option instead of combining every
variant. Choose only the version or component you want; selected alternatives
remain separate library entries.

Required components can also ship together in one archive. Put a
`zcom-mod.json` beside each component's payload; the review applies the nearest
manifest to that component and offers **Install all components**. On upgrade,
each component is matched to the installed payload it replaces, even if the old
components originally came from separate archives.

### PAK-only mods

A conventional `Example_P.pak` payload is supported and deployed to `~mods`.

### UE4SS mods

A UE4SS mod is a folder the runtime loads by name, holding either payload:

```text
MyMod/Scripts/main.lua
MyMod/dlls/main.dll
```

Both are recognized, in any capitalization, at any nesting depth. An archive
that ships several mod folders installs each as its own entry, so they can be
enabled, ordered, and removed separately. UE4SS starts mods in the order
`mods.txt` lists them, and that order is editable on the Load order tab; the
runtime's own entries and comments keep their place. UE4SS must already have a healthy Zero Company
layout. ZCOM Mod Manager does not redistribute UE4SS; drop a downloaded runtime
package on the installer and it is recognized as the runtime rather than as a
mod.

### Game-folder mods

Mods that the game reads from its own folders are installed from three
recognized layouts:

```text
AnyFolder/SWZeroCompany/Content/Movies/Intro.mp4   deployed relative to the game
LogicMods/Blueprint_P.pak                          deployed to Content/Paks/LogicMods
ReShade/dxgi.dll + ReShade.ini + shaders           deployed to Binaries/Win64
```

This is the only mod type that replaces an existing file. The original is kept
in the managed library and restored when the mod is disabled or uninstalled,
and a file another installed mod owns is never overwritten.

### Updating a mod

Installing a newer build of a mod you already have is recognized at inspection:
the preview offers to replace the installed version rather than reporting a
deployment conflict. The previous version's files are moved aside, not deleted,
so a failed upgrade puts them back and leaves the old version installed. The
replacement keeps its predecessor's position in the load order.

### Naming

A mod is named after the download it came from, with Nexus publishing metadata
removed, so `ZCUnlocked 34 1.3 2026-08-30T07-32Z i9WZfkaQ7.zip` installs as
"ZC Unlocked" at version 1.3. Names are editable before installing and can be
changed later from the library; renaming never touches deployed file names.

### Migrating existing mods

When ZCOM first connects to a game installation, it checks the controlled mod
folders for packages installed by hand or by another manager. The same scan is
always available from **Mods → Discover existing mods**.

PAK/IoStore container families, UE4SS mod folders, and additive `LogicMods` can
be adopted. Review the candidates, optionally merge container families that
belong to one download, edit their names, and select what ZCOM should manage.
Adoption copies each payload into the managed library and records checksums;
the live files, filenames, load order, and `mods.txt` are not changed.

Known UE4SS runtime components are shown but unchecked. Replacement-style
game-folder mods such as ReShade are reported but cannot be adopted safely,
because ZCOM did not see and back up the original file they replaced.

## Installation

Download the package for your platform from the GitHub release:

- Linux: make the AppImage executable and run it, or install the `.deb`.
- Windows: run the NSIS installer, or extract the portable `.zip` anywhere and
  run `ZCOM Mod Manager.exe`. Keep `retoc.exe` beside it: the portable build
  cannot verify or repack IoStore containers without the sidecar. The portable
  build also assumes the Microsoft Edge WebView2 runtime is already present,
  which it is on Windows 11 and on Windows 10 machines with current Edge; the
  installer downloads it when missing. Community builds are unsigned, so
  Windows SmartScreen may show a warning. Verify the release checksum and
  repository before choosing **Run anyway**.

Release packages include the MIT-licensed retoc 0.1.5 sidecar. 7z files use the
system `7z` command: install `p7zip`/`7zip` if it is not already available.

## Quick Start

1. Start ZCOM Mod Manager.
2. Confirm the automatically detected game path, or choose **Locate game**.
3. Open **Install** and drop a downloaded mod archive onto the window.
4. Review detected files, verification, compatibility, and conflicts.
5. Choose **Install**.

By default, **Launch game** uses Steam. To use a different executable or
launcher, select it under **Settings → Game installation**, save the setting,
and launch from Home. **Use Steam** clears the override.

The manager makes one lightweight request to the project’s latest GitHub
release endpoint when it opens. If a newer manager release exists, an update
icon appears beside **About**. Offline failures do not block startup, mod
management, or game launch.

Instead of manually extracting `.pak`, `.ucas`, and `.utoc` into
`SWZeroCompany/Content/Paks/~mods`, drop the downloaded archive into the app.
The `~mods` directory is created automatically.

## Installing a Mod

Archives are extracted into a unique cache sandbox. Absolute paths, `..` path
traversal, and symbolic links are rejected. Executables, scripts, and DLLs are
never run. Only recognized packaged-mod or Lua payload files are copied into
the managed library and then deployed.

Installation follows: extract → recognize → validate → stage → deploy → commit
database ownership. If deployment or the database commit fails, newly deployed
files are removed.

## Enabling / Disabling Mods

Use the switch on **Mods**. Disabling a packaged mod removes only destinations
recorded for that mod and retains the managed source copy. Enabling redeploys
from that copy. UE4SS toggles also edit only the matching `mods.txt` entry;
comments, unrelated entries, indentation, and line endings are retained.

## Uninstalling Mods

Before deletion, the manager recalculates every deployed SHA-256 checksum. If a
file changed since deployment, uninstall stops and keeps it. The normal UI does
not offer a casual force-delete path; the safe default is always to preserve
unexpected user data.

## Conflict Detection

Two levels are tracked:

1. **Filesystem collision:** two payloads target the same destination. A new
   install never overwrites an existing file.
2. **Package collision:** retoc package identifiers are hashed and stored. Mods
   that override the same identifiers are reported as overlapping packages,
   even when container filenames differ.

Normal UI and logs show only overlap counts. Raw asset paths are exposed only
after the user enables advanced package names in Settings and opens Advanced
Details; those names can contain spoilers.

## Load Order

**Mods → Load order** lists supported packaged mods from highest to lowest
priority. Move a mod toward the top to make it win known package overlaps, then
review the exact deployment filenames before applying. Newly installed
runtime-supported packaged mods start at the highest priority.

The manager keeps original filenames in its source library and applies a
numeric patch rank only to deployed companions. For example,
`Example_P.pak/.utoc/.ucas` at priority 3 becomes
`Example_0003_P.pak/.utoc/.ucas`. Every current file is checksum-verified
before a rename. A failed filesystem or database step rolls back, and an
interrupted operation is recovered at the next startup.

IoStore triplets are orderable because both priority directions were
demonstrated against Zero Company's runtime and re-verified with retoc after
each rename. Pure UTOC/UCAS pairs and PAK-only mods remain visible but
non-orderable. The pair layout is untested; the PAK-only capability fixture did
not pass the runtime gate. PAK-only package contents also remain opaque, so
their overlap winners cannot be identified automatically.

## Container Verification

ZCOM Mod Manager invokes bundled or user-selected **retoc 0.1.5** using
`retoc verify <container.utoc>` and collects package identifiers with `retoc
list --package --path`. A failed or unavailable verifier prevents IoStore
installation. Tool output shown in normal mode has home-directory prefixes
replaced with `~` and is truncated to avoid accidental data disclosure.

## UE4SS Mods

The expected runtime is:

```text
SWZeroCompany/Binaries/Win64/
├── dwmapi.dll
└── ue4ss/
    ├── UE4SS.dll
    ├── UE4SS-settings.ini
    └── Mods/
```

UE4SS is only needed by mods that require it. Many Zero Company mods are plain
IoStore or PAK payloads and work without it.

### Getting UE4SS

**Home → Runtime readiness → UE4SS runtime** offers two actions:

1. **Get the tested build on Nexus Mods** opens
   <https://www.nexusmods.com/starwarszerocompany/mods/9> in your browser. That
   page hosts the UE4SS build tested against Zero Company.
2. **Install from downloaded package…** takes the ZIP or 7z you downloaded and
   unpacks it into `SWZeroCompany/Binaries/Win64`.

The archive goes through the same sandbox as mod archives: absolute paths, `..`
traversal, and symbolic links are rejected, and nothing is executed. On a
reinstall or upgrade, your configuration is preserved rather than overwritten:

- `ue4ss/UE4SS-settings.ini`
- `ue4ss/Mods/mods.txt` and `ue4ss/Mods/mods.json`
- every `load_order.txt` under `ue4ss/Mods/`

Lua mods you installed yourself need no rule: a package does not contain them,
and a file the package does not contain is never touched. Lua mods the package
*does* ship (`BPModLoaderMod`, `ConsoleCommandsMod`, and friends) belong to the
runtime and are updated with it, so an upgraded `UE4SS.dll` is never left
paired with stale scripts.

The manager reports how many files it wrote and which it kept. To adopt a
shipped `UE4SS-settings.ini`, rename or delete your copy and install again.

Either flow also works through the Nexus Mods handoff described below.

## Linux / Proton

Steam libraries under `~/.local/share/Steam`, `~/.steam/steam`, and every path
in `steamapps/libraryfolders.vdf` are scanned. The manager also checks for
`steamapps/compatdata/2075800`.

When UE4SS is present but no matching launch option can be detected, add this
to the game's Steam launch options:

```text
WINEDLLOVERRIDES="dwmapi=n,b" %command%
```

The application never edits Steam launch options. Flatpak Steam libraries may
require manual game selection and appropriate filesystem permissions.

## Windows

Steam is not assumed to be on `C:`. Common install roots and configured library
folders are scanned. The selected directory must contain both
`SWZeroCompany/Binaries/Win64/SWZeroCompany.exe` and
`SWZeroCompany/Content/Paks/`.

Unsigned builds can trigger SmartScreen. Code signing can be added later
through standard Tauri signing secrets without changing application behavior.

## Diagnostics

**Mod Doctor** checks the game layout, manifest/build, `~mods`, owned mods,
package conflicts, retoc, UE4SS, compatdata, and the Proton DLL override. The
report is copyable and home-directory paths are sanitized. Structured JSONL
logs are available from Settings → **Open logs folder**.

## Nexus Mods Handoff

The manager does not browse, search, or scrape Nexus Mods, and it never starts a
download on its own. Downloads begin where they are meant to: on the website.

1. In **Settings → Nexus Mods downloads**, paste a personal API key from
   <https://www.nexusmods.com/users/myaccount?tab=api>. The key is verified
   against Nexus before it is stored.
2. Enable **Handle `nxm://` links from the browser**. Nothing touches your
   desktop configuration or registry until you do, and turning it off hands the
   association back to whatever held it before.
3. On any Zero Company mod page, press **Mod Manager Download**. The browser
   passes the link over, the file is fetched with a progress readout, and it
   lands in the same review screen as a mod you picked by hand — same
   validation, same conflict checks, same confirmation before anything deploys.

A link for any other game is refused rather than downloaded.

### Update checking

A download through the handoff records which Nexus mod and file it came from,
and installation attaches that to the mod. **Check for updates** on the Mods
page then asks Nexus what each of those mods now offers, and marks the ones with
a newer file.

A library installed before any of that is not left out. The same check offers
the MD5 of each unmatched mod's archive to Nexus, which recognises the file it
was uploaded as and identifies the mod and file exactly — no download and no
guessing from names. An archive Nexus does not recognise is remembered, so an
automatic check does not ask about it again.

A mod whose archive is gone, or that was adopted from the game folder and never
had one, can be pointed at its Nexus page by hand: open **More details** and
paste the mod's address. The file recorded as installed is the one carrying the
installed version, so linking never invents an update.

A linked mod can be opened on Nexus from its row in the library or from **More
details**, so its description, changelog, and comments are one click away.

**More details** is also where a mod leaves checking for good. That matters for
anything not published on Nexus — a mod you built yourself, or one from
elsewhere — because its archive is otherwise offered to the lookup on every
check you ask for. An excluded mod is left out of the checks and the lookup
alike, and **Check this mod again** puts it back.

An update is the newest file still offered under `MAIN` or `UPDATE`. Superseded,
archived, and deleted files are ignored, an optional extra is never treated as
an upgrade, and newer is decided by file id, which Nexus issues in upload order.

A premium account can fetch the update from the Mods page; it lands in the same
review screen and replaces the installed mod through the same path a website
handoff uses. A free account is sent to the mod's files tab, because only the
website can mint the key the download link needs.

Nothing is checked unless you ask. **Settings → Nexus Mods downloads** offers an
opt-in start-up check, off by default, and its result stands for six hours so
reopening the manager does not spend your API allowance.

If another mod manager already holds `nxm://`, Settings names it rather than
failing silently, and the switch takes the association over. Turning the switch
off hands it back.

Linux desktop associations have two traps that this application works around,
both of which otherwise fail without any error message. `xdg-mime` resolves a
desktop entry by passing the first whitespace-separated word of `Exec` to
`command -v` without removing quotes, so a quoted path is never resolved; the
entry is written unquoted, using a symbolic link when the real path needs
quoting. And `xdg-mime query` reads `<desktop>-mimeapps.list` before the
generic `mimeapps.list` while `xdg-mime default` only writes the generic one,
so a scheme claimed in the prefixed file can never be taken over by
registering normally; the prefixed files are updated as well.

### Where the API key is kept

The key is stored in the operating system's secret store: GNOME Keyring or
KWallet through the Secret Service, or Credential Manager on Windows. If no
secret store is available — common on a minimal Linux install — the key falls
back to the application database in plain text and **Settings says so**, rather
than implying a protection that is not there.

The key can download on your behalf and is rate-limited against your account.
Treat it like a password: it is never logged, never sent anywhere except
`api.nexusmods.com`, and **Remove stored key** clears both locations.

### Why a key is required at all

A non-premium Nexus account cannot obtain a download link from the API alone.
The `key` and `expires` pair that authorises the download is minted by the
website when you press **Mod Manager Download**, which is precisely why the
handoff exists rather than an in-application browser.

## Optional `zcom-mod.json` Manifest

Existing mods do not need a manifest. Authors can provide metadata:

```json
{
  "schemaVersion": 1,
  "id": "community.example.cheaper-actions",
  "name": "Cheaper Actions",
  "version": "1.0.0",
  "author": "Example Author",
  "description": "Adjusts action economy values.",
  "game": {
    "appId": 2075800,
    "testedBuilds": ["24874058"]
  },
  "type": ["iostore"],
  "requires": { "ue4ss": false }
}
```

The versioned JSON Schema is [schema/zcom-mod.schema.json](schema/zcom-mod.schema.json).
Unknown properties are allowed for forward-compatible community extensions.

## Building From Source

Requirements:

- Node.js 22+
- Rust stable
- Tauri 2 Linux prerequisites (`webkit2gtk-4.1`, GTK 3, librsvg, patchelf)
- `7z` for 7z archive installation/tests

```bash
git clone https://github.com/arctco/zcom-mod-manager.git
cd zcom-mod-manager/zcom-mod-manager
npm ci
npm run prepare:retoc
npm run tauri build
```

`prepare:retoc` explicitly downloads the official upstream 0.1.5 archive when
no local `retoc` exists and verifies the publisher-provided SHA-256 file. Set
`RETOC_SOURCE=/trusted/path/to/retoc` to use an existing binary.

## Development

```bash
npm run typecheck
npm test
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

One test is ignored by default because it needs a real published package that
CI cannot download. To exercise the UE4SS installer end to end, download a
package from the mod page above and run:

```bash
ZCOM_UE4SS_ARCHIVE=/path/to/ue4ss-package.zip \
  cargo test --manifest-path src-tauri/Cargo.toml -- --ignored
```

It performs a fresh install, then an upgrade over edited configuration and a
user-supplied Lua mod, and asserts what is kept and what is replaced.

The application is offline-first. Use synthetic fixtures only—never add game
assets, package dumps, credentials, SDK dumps, or personal logs.

## Project Structure

```text
src/                         React/TypeScript UI
src-tauri/src/steam/         Steam and game discovery
src-tauri/src/archives/      sandboxed ZIP/7z staging
src-tauri/src/mods/          payload and manifest recognition
src-tauri/src/adoption.rs    existing-mod discovery and adoption
src-tauri/src/deployment/    ownership-safe lifecycle
src-tauri/src/retoc/         verifier abstraction
src-tauri/src/ue4ss/         runtime and mods.txt handling
src-tauri/src/database/      SQLite schema and queries
src-tauri/src/diagnostics/   Mod Doctor
schema/                      optional community manifest schema
```

## Release Builds

CI builds the production executable on every main-branch push and pull request.
Tags matching `v*` publish a GitHub release immediately and attach Linux
AppImage/deb and Windows NSIS installer/portable zip artifacts. The release is
not a draft, so bump the version in `package.json`, `package-lock.json`,
`src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`,
then smoke-test both platforms before tagging. The release workflow rejects a
tag that does not match those files.

```bash
npm run check:release-version -- v0.4.1
git tag -s v0.4.1 -m "ZCOM Mod Manager 0.4.1"
git push origin v0.4.1
```

Confirm checksums after the run finishes. See
[KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) before release.

## Roadmap

- **0.2:** conflict-aware packaged-mod load order, Steam launch, and automatic
  manager release notices shipped; profiles and dependency metadata remain
- **0.3:** Nexus handoff shipped; per-mod update checking has since shipped
  too, and Nexus catalog browsing remains out of scope
- **0.4:** existing-mod migration, bundled packaged-variant selection, and a
  custom game executable shipped
- **Future / separate project:** ZCOM Mod Studio for asset inspection and authoring

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup,
architecture, tests, privacy expectations, and adding a mod format. Security
issues involving archive handling or deletion safety should not be disclosed
with real user paths or game data.

## Credits

Thanks to the Zero Company modding community and to the maintainers of retoc, Tauri, React, rusqlite, zip-rs, and the wider open-source ecosystem.

## Third-Party Software

retoc is MIT-licensed and is bundled unmodified in release packages. Exact
copyright and license notices are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
No code was copied from Vortex or another mod manager.

## License

ZCOM Mod Manager is licensed under the
[GNU General Public License version 3 only (`GPL-3.0-only`)](LICENSE).
Copyright © 2026 Victor Hugo (arctco).

The license permits forks and redistribution, but it does not grant permission
to present a modified build as an official ZCOM Mod Manager release. See the
[trademark and branding policy](TRADEMARKS.md) for use of the project name and
logo.

## Disclaimer

Modding can make saves or game installations unstable. Back up important data,
read each mod's documentation, and review compatibility after every game
update. This project does not provide game files, UE SDK data, or copyrighted
assets and does not bypass ownership or platform protections.
