# Third-Party Notices

ZCOM Mod Manager's application code and original icon are licensed under the
GNU General Public License version 3 only (`GPL-3.0-only`). The copyright
license does not grant trademark rights in the project name or logo; see
`TRADEMARKS.md`. No game files, game assets, UE4SS binaries, or source code from
another mod manager are included.

## retoc

- Project: retoc 0.1.5
- Repository: https://github.com/trumank/retoc
- License: MIT
- Use: official release binaries are bundled in release packages as a Tauri
  sidecar and invoked for IoStore verification and package-ID enumeration.
- Copyright: Copyright (c) 2025 Truman Kilen and Archengius
- Required notice: the upstream MIT license text is reproduced below.

> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

## Rust and JavaScript dependencies

The project uses Tauri, React, Vite, rusqlite, zip-rs, and other packages listed
exactly in `package-lock.json` and `src-tauri/Cargo.lock`. They are consumed as
unmodified dependencies under their respective permissive/open-source
licenses. Release workflows preserve both lockfiles. Run `cargo license` and
`npx license-checker` when preparing a release if those optional audit tools
are installed.
