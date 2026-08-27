#!/usr/bin/env bash
# Builds a .deb of the GTK shell.
#
#   ./deb.sh 0.4.0            → prchum_0.4.0_<arch>.deb
#
# Hand-rolled rather than cargo-deb: the whole package is a staged tree
# plus a control file, and doing it directly means the dependencies say
# what they actually are instead of what a generator guessed.
set -euo pipefail

VERSION="${1:?usage: deb.sh <version>}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${2:-$HERE/../target/release/prchum-gtk}"

case "$(uname -m)" in
    x86_64) ARCH=amd64 ;;
    aarch64 | arm64) ARCH=arm64 ;;
    *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

# Output lands in dist/ rather than the current directory: it is a build
# product, ignored by git, and nothing else should have to step over it.
OUTDIR="${OUTDIR:-$HERE/dist}"
mkdir -p "$OUTDIR"

BUILD="$(mktemp -d)"
trap 'rm -rf "$BUILD"' EXIT
ROOT="$BUILD/root"

"$HERE/stage.sh" "$ROOT" "$BINARY"

# Depends is the runtime GTK stack, which the app links against.
# git is Recommends rather than Depends: prchum opens patch files
# without it, and a hard dependency would be a lie about what it needs.
# The forge CLIs are Suggests — you install the one for your forge, and
# prchum stores no credentials of its own precisely so that they can.
mkdir -p "$ROOT/DEBIAN"
cat > "$ROOT/DEBIAN/control" <<EOF
Package: prchum
Version: $VERSION
Section: devel
Priority: optional
Architecture: $ARCH
Depends: libgtk-4-1 (>= 4.12), libadwaita-1-0 (>= 1.5), libc6
Recommends: git
Suggests: gh, glab
Maintainer: Horacio Duran <horacio.duran@gmail.com>
Homepage: https://github.com/perrito666/prchum
Description: Review pull requests and patches
 Prchum reviews code: pull requests from GitHub, Forgejo and GitLab, git
 comparisons, and plain patch files. Comments anchor to a semantic
 location rather than to a position in a diff, so a draft survives the
 branch moving under it.
 .
 It never asks for a token. Forges are reached through the command line
 tools you have already authenticated, so credentials stay where you
 put them.
EOF

# Refresh the desktop and icon caches, so the app appears without a
# logout. Failure is not fatal: on a machine with no desktop installed
# there is nothing to refresh.
cat > "$ROOT/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "configure" ]; then
    update-desktop-database -q /usr/share/applications 2>/dev/null || true
    gtk-update-icon-cache -qtf /usr/share/icons/hicolor 2>/dev/null || true
fi
EOF
chmod 755 "$ROOT/DEBIAN/postinst"

cp "$ROOT/DEBIAN/postinst" "$ROOT/DEBIAN/postrm"

OUT="$OUTDIR/prchum_${VERSION}_${ARCH}.deb"
dpkg-deb --root-owner-group --build "$ROOT" "$OUT" >/dev/null
echo "$OUT"
