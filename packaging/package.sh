#!/usr/bin/env bash
# Packages a release build:
#   packaging/package.sh <linux-x86_64|windows-x86_64|macos-universal|macos> [version]
# Result in dist/. On a Mac, after `cargo build --release -p xcopy-modern`:
#   bash packaging/package.sh macos
set -euo pipefail
name="$1"
version="${2:-dev}"
app="X-COPY Modern"
bin_name="xcopy-modern"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
mkdir -p dist
dir="X-COPY-Modern-$version-$name"
rm -rf "$dir"

case "$name" in
  linux-*)
    mkdir -p "$dir/icons"
    cp "target/release/$bin_name" "$dir/$bin_name"
    for s in 16 24 32 48 64 128 256; do cp "packaging/icon-$s.png" "$dir/icons/$s.png"; done
    cp packaging/icon.png "$dir/icons/512.png"
    cp packaging/linux/xcopy-modern.desktop packaging/linux/install.sh README.md LICENSE "$dir/"
    chmod +x "$dir/$bin_name" "$dir/install.sh"
    tar czf "dist/$dir.tar.gz" "$dir"
    ;;
  windows-*)
    mkdir -p "$dir"
    cp "target/release/$bin_name.exe" "$dir/X-COPY-Modern.exe"
    cp README.md LICENSE "$dir/"
    cat > "$dir/GREASEWEAZLE.txt" <<'TXT'
X-COPY Modern needs the Greaseweazle host tools (gw.exe).

Unzip the official Windows release of the Greaseweazle tools
(https://github.com/keirf/greaseweazle/releases) into this folder:
    X-COPY-Modern.exe
    greaseweazle-x.y\gw.exe
X-COPY Modern finds it automatically (also on the PATH and in Python
"Scripts" folders). Otherwise click SETUP and paste the path of gw.exe.
TXT
    7z a -tzip "dist/$dir.zip" "$dir" > /dev/null
    ;;
  macos-*)
    bundle="$dir/$app.app"
    mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
    # Universal binary from the release workflow, or a plain `cargo build --release` on a Mac.
    bin="target/universal/$bin_name"
    [ -x "$bin" ] || bin="target/release/$bin_name"
    cp "$bin" "$bundle/Contents/MacOS/$bin_name"
    sed "s/@VERSION@/${version#v}/g" packaging/macos/Info.plist > "$bundle/Contents/Info.plist"
    cp packaging/icon.icns "$bundle/Contents/Resources/AppIcon.icns"
    cp README.md LICENSE "$dir/"
    ditto -c -k --keepParent "$dir" "dist/$dir.zip"
    ;;
  *)
    echo "unknown target: $name" >&2
    exit 1
    ;;
esac
rm -rf "$dir"
ls -l dist
