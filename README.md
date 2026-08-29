<div align="center">
  <img src="images/icon.png" alt="" width="160" height="160">
  <h1>ZCOM Mod Manager</h1>
</div>

A dedicated open-source mod manager for **Star Wars: Zero Company**.

ZCOM Mod Manager understands Zero Company mod payloads instead of
treating them as arbitrary files. It discovers Steam installations, validates
IoStore containers with retoc, manages UE4SS Lua mods, detects package overlap,
records SHA-256 ownership, and treats Linux/Proton as a first-class platform.

> This is an independent community project. It is not affiliated with or
> endorsed by Electronic Arts, Lucasfilm, Disney, Bit Reactor, or Nexus Mods.
> Star Wars and related names are trademarks of their respective owners.

## Features

- Steam AppID `2075800` discovery across default and additional libraries
- Dynamic Steam build-ID detection and update warnings
- ZIP, 7z, direct PAK/UTOC/UCAS, folder, picker, and drag-and-drop input
- Nested archive payload discovery without copying documentation or random data
- IoStore pair/triplet validation and retoc 0.1.5 verification
- PAK-only and UE4SS `Scripts/main.lua` mod support
- Manager-owned source library plus checksum-guarded deployment records
- Enable, disable, verify, and safe uninstall operations
- Filename and hashed IoStore package conflict detection
- Optional spoiler-sensitive package paths, disabled by default
- UE4SS layout checks and formatting-preserving `mods.txt` updates
- Guided UE4SS runtime installation from a package you downloaded yourself
- Linux compatdata and Proton DLL-override diagnostics
- Sanitized structured logs and a copyable Mod Doctor report
- No account, network requirement, telemetry, analytics, or advertisements

## Screenshots

The application ships a restrained tactical dark interface with five primary
areas: Home, Mods, Install, Diagnostics, and Settings. Release screenshots are
kept in `docs/screenshots/` when captured from a tagged build.

The palette (ceramic white, maroon-brown linework, 212th orange, and sage and
coral accents) and the application icon are derived from community clone
trooper helmet artwork supplied by the project owner. The interface contains no
extracted game assets. If you fork this project, replace `images/`,
`src/assets/helmet.png`, and `src-tauri/icons/` with artwork you have the right
to redistribute.

## Supported Platforms

| Platform | Architecture | Packages |
| --- | --- | --- |
| Linux | x86_64 | AppImage and `.deb` |
| Windows 10/11 | x86_64 | NSIS `.exe` and MSI |

Steam Deck/SteamOS should work through the x86_64 Linux AppImage. Add it as a
non-Steam application if desired. See [Known Limitations](KNOWN_LIMITATIONS.md)
for the boundaries of v0.1.0.

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

### PAK-only mods

A conventional `Example_P.pak` payload is supported and deployed to `~mods`.

### UE4SS Lua mods

Both `Scripts/main.lua` and `scripts/main.lua` layouts are recognized. UE4SS
must already have a healthy Zero Company layout. ZCOM Mod Manager does not
redistribute UE4SS.

## Installation

Download the package for your platform from the GitHub release:

- Linux: make the AppImage executable and run it, or install the `.deb`.
- Windows: run the NSIS installer or MSI. v0.1.0 community builds are unsigned,
  so Windows SmartScreen may show a warning. Verify the release checksum and
  repository before choosing **Run anyway**.

Release packages include the MIT-licensed retoc 0.1.5 sidecar. 7z files use the
system `7z` command: install `p7zip`/`7zip` if it is not already available.

## Quick Start

1. Start ZCOM Mod Manager.
2. Confirm the automatically detected game path, or choose **Locate game**.
3. Open **Install** and drop a downloaded mod archive onto the window.
4. Review detected files, verification, compatibility, and conflicts.
5. Choose **Install**.

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

Unsigned v0.1.0 builds can trigger SmartScreen. Code signing can be added later
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
git clone https://github.com/zcom-modding/zcom-mod-manager.git
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
AppImage/deb and Windows NSIS/MSI artifacts. The release is not a draft, so
smoke-test both platforms before tagging.

```bash
git tag -s v0.1.1 -m "ZCOM Mod Manager 0.1.1"
git push origin v0.1.1
```

Confirm checksums after the run finishes. See
[KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) before release.

## Roadmap

- **0.2:** profiles, load-order tools, dependency metadata, mod update checks
- **0.3:** *(in progress)* Nexus handoff shipped; update checking against the
  API still to come
- **Future / separate project:** ZCOM Mod Studio for asset inspection and authoring

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup,
architecture, tests, privacy expectations, and adding a mod format. Security
issues involving archive handling or deletion safety should not be disclosed
with real user paths or game data.

## Credits

Thanks to the Zero Company modding community and to the maintainers of retoc,
Tauri, React, rusqlite, zip-rs, and the wider open-source ecosystem.

## Third-Party Software

retoc is MIT-licensed and is bundled unmodified in release packages. Exact
copyright and license notices are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
No code was copied from Vortex or another mod manager.

## License

ZCOM Mod Manager is available under the [MIT License](LICENSE).

## Disclaimer

Modding can make saves or game installations unstable. Back up important data,
read each mod's documentation, and review compatibility after every game
update. This project does not provide game files, UE SDK data, or copyrighted
assets and does not bypass ownership or platform protections.
