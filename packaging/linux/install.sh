#!/usr/bin/env bash
# Installs X-COPY Modern for the current user: program, menu entry and icons.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bin="$HOME/.local/bin"
apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor"
mkdir -p "$bin" "$apps"
install -m 755 "$here/xcopy-modern" "$bin/xcopy-modern"
for f in "$here"/icons/*.png; do
  s="$(basename "$f" .png)"
  mkdir -p "$icons/${s}x${s}/apps"
  cp "$f" "$icons/${s}x${s}/apps/xcopy-modern.png"
done
sed "s|^Exec=.*|Exec=$bin/xcopy-modern|" "$here/xcopy-modern.desktop" > "$apps/xcopy-modern.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$apps" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$icons" || true
echo "Installed: $bin/xcopy-modern (menu entry: X-COPY Modern)."
echo "Add yourself to the 'dialout' group for USB access: sudo usermod -aG dialout \$USER"
