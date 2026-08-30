# Contributing to ZCOM Mod Manager

Thank you for helping build a safe community modding foundation.

## Development setup

Install Node.js 22+, stable Rust, Tauri 2 platform prerequisites, and 7z. Run
`npm ci`, `npm run prepare:retoc`, and `npm run tauri dev`. On Windows use
`scripts/prepare-retoc.ps1` from PowerShell.

## Architecture

React renders state and user intent. It does not deploy files. Tauri commands
are a narrow serialization boundary. Rust owns Steam discovery, archive input,
recognition, verification, SQLite, checksums, deployment, conflicts, UE4SS, and
diagnostics. Keep modules small and error variants actionable.

The database is migration-based from schema version 1. New schema changes must
be additive or include an explicit migration and an upgrade test. Do not make
the game directory the only copy of a mod.

## Code style

- TypeScript is strict; avoid `any` and keep backend payload types in `types.ts`.
- Run `cargo fmt`; all Clippy warnings are CI errors.
- Prefer typed `AppError` variants to string-only backend failures.
- Keep user-facing text spoiler-conscious and paths sanitized by default.
- Preserve keyboard focus, 4.5:1 text contrast, reduced-motion behavior, and
  semantic status text in UI changes.

## Testing

Run before opening a pull request:

```bash
npm run typecheck
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-features
```

Tests must use temporary directories and synthetic bytes. Never commit extracted
game packages, real UTOCs, `.usmap` files, logs, Steam data, or absolute paths.

## Pull requests

Explain the user problem, safety impact, tests, and platforms exercised. Keep
unrelated formatting changes out of the patch. UI changes should include a
screenshot captured with synthetic data. Deployment changes require rollback,
checksum mismatch, and unknown-file tests.

## Adding another mod format

1. Define an unambiguous payload recognizer in `src-tauri/src/mods/`.
2. Reject incomplete or mixed payloads before staging.
3. Define the exact allowed deployment root; never honor an archive's absolute
   destination or execute an included installer.
4. Store source-relative and destination paths, sizes, SHA-256 hashes, and owner.
5. Add enable, disable, uninstall, rollback, collision, and checksum tests.
6. Add diagnostics and concise UI labels.
7. Update README, schema (if applicable), changelog, and third-party notices.

## Reporting game-update incompatibility

Open an issue with the Steam build ID, ZCOM Mod Manager version, mod type, and a
sanitized Mod Doctor report. Do not attach copyrighted game assets, raw package
lists, home-directory paths, save files, or Steam credentials. State whether
the issue reproduces with all mods disabled.

## Licensing

Contributions are accepted under the GNU General Public License v3.0. Do not paste code from proprietary or
license-unclear projects. Any adapted code must identify the upstream project,
exact license, changed files, and required notice in the pull request and
`THIRD_PARTY_NOTICES.md`.
