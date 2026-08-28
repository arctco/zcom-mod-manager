#!/usr/bin/env bash
set -euo pipefail

version="0.1.5"
target="$(rustc -vV | sed -n 's/^host: //p')"
output="src-tauri/binaries/retoc-${target}"
mkdir -p src-tauri/binaries

if [[ -n "${RETOC_SOURCE:-}" ]]; then
  cp "$RETOC_SOURCE" "$output"
elif command -v retoc >/dev/null 2>&1; then
  cp "$(command -v retoc)" "$output"
else
  case "$target" in
    x86_64-unknown-linux-gnu) archive="retoc_cli-x86_64-unknown-linux-gnu.tar.xz" ;;
    *) echo "No automated retoc preparation is defined for $target" >&2; exit 1 ;;
  esac
  temporary="$(mktemp -d)"
  trap 'rm -rf "$temporary"' EXIT
  base="https://github.com/trumank/retoc/releases/download/v${version}"
  curl --fail --location --proto '=https' --tlsv1.2 "$base/$archive" --output "$temporary/$archive"
  curl --fail --location --proto '=https' --tlsv1.2 "$base/$archive.sha256" --output "$temporary/$archive.sha256"
  (cd "$temporary" && sha256sum --check "$archive.sha256")
  tar -xJf "$temporary/$archive" -C "$temporary"
  found="$(find "$temporary" -type f -name retoc -perm -u+x -print -quit)"
  test -n "$found"
  cp "$found" "$output"
fi

chmod 0755 "$output"
"$output" --version
