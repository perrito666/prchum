#!/usr/bin/env bash
# Lays out the files a Linux package installs, into a staging root.
#
#   ./stage.sh <staging-root> [binary]
#
# Every packager here — deb, rpm, Arch — installs the same tree, so it is
# described once. The binary defaults to the release build; the Arch
# PKGBUILD passes its own, because makepkg builds in its own directory.
set -euo pipefail

ROOT="${1:?usage: stage.sh <staging-root> [binary]}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA="$HERE/../data"
BINARY="${2:-$HERE/../target/release/prchum-gtk}"

[[ -f "$BINARY" ]] || { echo "no binary at $BINARY — build it first" >&2; exit 1; }

install -Dm755 "$BINARY" "$ROOT/usr/bin/prchum-gtk"

install -Dm644 "$DATA/eu.dumontix.prchum.desktop" \
    "$ROOT/usr/share/applications/eu.dumontix.prchum.desktop"

# AppStream metadata: what a software centre reads to show a name, a
# summary and an icon. Not decoration — without it the app is a bare
# entry in a menu.
install -Dm644 "$DATA/eu.dumontix.prchum.metainfo.xml" \
    "$ROOT/usr/share/metainfo/eu.dumontix.prchum.metainfo.xml"

for size in 128 256 512; do
    install -Dm644 "$DATA/icons/eu.dumontix.prchum-$size.png" \
        "$ROOT/usr/share/icons/hicolor/${size}x${size}/apps/eu.dumontix.prchum.png"
done

install -Dm644 "$HERE/../../LICENSE" "$ROOT/usr/share/doc/prchum/LICENSE"
