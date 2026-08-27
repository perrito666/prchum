#!/usr/bin/env bash
# Writes a release's version into the files that state one.
#
#   ./stamp.sh 0.5.0
#
# Called by the release workflow, from the tag, so that tagging is the
# only thing anybody has to do. The alternative — a version to bump by
# hand before every tag — is a step that gets forgotten, and a PKGBUILD
# that quietly builds the previous release is worse than one that says
# it is out of date.
#
# The repository's copies stay at whatever was last stamped into them.
# The authoritative PKGBUILD is the one attached to each release.
set -euo pipefail

VERSION="${1:?usage: stamp.sh <version>}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$HERE/../.."
TODAY="${STAMP_DATE:-$(date -u +%Y-%m-%d)}"

# --- the Arch recipe ----------------------------------------------------

PKGBUILD="$HERE/PKGBUILD"
sed -i.bak "s/^pkgver=.*/pkgver=$VERSION/" "$PKGBUILD"
sed -i.bak "s/^pkgrel=.*/pkgrel=1/" "$PKGBUILD"

# A real checksum rather than SKIP, so makepkg verifies what it fetched.
# Only possible here, where the tag exists and its tarball can be read.
TARBALL_URL="https://github.com/perrito666/prchum/archive/refs/tags/v$VERSION.tar.gz"
if SUM=$(curl -fsSL "$TARBALL_URL" | sha256sum | cut -d' ' -f1) && [ -n "$SUM" ]; then
    sed -i.bak "s/^sha256sums=.*/sha256sums=('$SUM')/" "$PKGBUILD"
else
    echo "warning: could not read $TARBALL_URL; leaving sha256sums alone" >&2
fi

# --- what a software centre reads --------------------------------------

METAINFO="$REPO/linux/data/eu.dumontix.prchum.metainfo.xml"
if ! grep -q "<release version=\"$VERSION\"" "$METAINFO"; then
    # Newest first, which is the order AppStream expects.
    python3 - "$METAINFO" "$VERSION" "$TODAY" <<'PYTHON'
import sys

path, version, today = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path) as handle:
    text = handle.read()

entry = (
    f'    <release version="{version}" date="{today}">\n'
    f'      <description>\n'
    f'        <p>See the release notes at the project\'s homepage.</p>\n'
    f'      </description>\n'
    f'    </release>\n'
)
marker = "  <releases>\n"
assert marker in text, "no <releases> block to add to"
with open(path, "w") as handle:
    handle.write(text.replace(marker, marker + entry, 1))
PYTHON
fi

rm -f "$PKGBUILD.bak"

echo "stamped $VERSION"
grep -E '^(pkgver|pkgrel|sha256sums)=' "$PKGBUILD"
