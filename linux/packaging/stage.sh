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

# `prchum` is the name to type. The binary keeps its own so the desktop
# entry and the window class stay stable, and this is a link rather than
# a wrapper because there is nothing to wrap: the app parses the same
# arguments the command documents.
ln -sf prchum-gtk "$ROOT/usr/bin/prchum"

# `git prchum`, which works because git treats any git-* on the PATH as
# a subcommand.
install -Dm755 "$HERE/../../scripts/git-prchum" "$ROOT/usr/bin/git-prchum"

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

# Man pages, compressed: Debian policy requires it, and rpm leaves an
# already-compressed page alone.
for page in prchum git-prchum; do
    install -Dm644 "$HERE/../../scripts/man/$page.1" \
        "$ROOT/usr/share/man/man1/$page.1"
    gzip -9n "$ROOT/usr/share/man/man1/$page.1"
done
