#!/usr/bin/env bash
set -euo pipefail

# linuxdeploy bundles the build host's Wayland client libraries into the AppDir,
# and the AppRun puts them ahead of the host's on the library search path. On a
# distribution whose Mesa is newer than the build host's Wayland (Ubuntu 24.04
# ships 1.22), loading libEGL_mesa then fails with "undefined symbol:
# wl_fixes_interface", WebKit reports "Could not create default EGL display:
# EGL_BAD_PARAMETER" and its web process aborts, leaving a blank window.
# These libraries are tied to the running compositor and driver stack, so they
# have to come from the host: drop them and repack the AppImage.

appimage="${1:?usage: fix-appimage.sh <path-to-AppImage>}"
appimage="$(readlink -f "$appimage")"
test -f "$appimage"

temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT

appimagetool="$temporary/appimagetool"
if command -v appimagetool >/dev/null 2>&1; then
  appimagetool="$(command -v appimagetool)"
else
  curl --fail --location --proto '=https' --tlsv1.2 \
    "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage" \
    --output "$appimagetool"
  chmod 0755 "$appimagetool"
fi

(cd "$temporary" && "$appimage" --appimage-extract >/dev/null)
appdir="$temporary/squashfs-root"
test -d "$appdir"

removed=0
while IFS= read -r library; do
  rm -f "$library"
  echo "Removed bundled $(basename "$library")"
  removed=$((removed + 1))
done < <(find "$appdir/usr/lib" -maxdepth 2 -name 'libwayland-*.so.*' -type f)

if [[ "$removed" -eq 0 ]]; then
  echo "No bundled Wayland libraries found in $appimage; leaving it untouched"
  exit 0
fi

ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$appimagetool" --no-appstream "$appdir" "$appimage"
chmod 0755 "$appimage"
echo "Repacked $appimage without $removed host-provided Wayland librar$([[ "$removed" -eq 1 ]] && echo y || echo ies)"
