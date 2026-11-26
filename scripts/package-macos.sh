#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p agentlaunch-slint
release_dir="${CARGO_TARGET_DIR:-target}/release"
app="$release_dir/AgentLaunch.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$release_dir/agentlaunch-slint" "$app/Contents/MacOS/AgentLaunch"

icon_tmp=$(mktemp -d)
trap 'rm -rf "$icon_tmp"' EXIT
iconset="$icon_tmp/AgentLaunch.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" assets/icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
python3 - "$iconset" "$app/Contents/Resources/AgentLaunch.icns" <<'PY'
from pathlib import Path
import struct
import sys

# ICNS uses big-endian chunk sizes (including the 8-byte header).
def chunk(kind, data):
    return struct.pack(">4sI", kind, len(data) + 8) + data

icons = [(b"icp4", "16x16"), (b"ic11", "16x16@2x"),
         (b"icp5", "32x32"), (b"ic12", "32x32@2x"),
         (b"ic07", "128x128"), (b"ic13", "128x128@2x"),
         (b"ic08", "256x256"), (b"ic14", "256x256@2x"),
         (b"ic09", "512x512"), (b"ic10", "512x512@2x")]
folder = Path(sys.argv[1])
images = [chunk(kind, (folder / f"icon_{size}.png").read_bytes()) for kind, size in icons]
Path(sys.argv[2]).write_bytes(chunk(b"icns", b"".join(images)))
PY
cp assets/macos/Info.plist "$app/Contents/Info.plist"
codesign --force --sign - "$app"
printf '%s\n' "$app"
