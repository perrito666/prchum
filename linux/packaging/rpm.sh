#!/usr/bin/env bash
# Builds an .rpm of the GTK shell.
#
#   ./rpm.sh 0.4.0            → prchum-0.4.0-1.<arch>.rpm
#
# rpmbuild wants a spec and a build root; the spec here installs the same
# staged tree the other packagers do, so the three cannot drift.
set -euo pipefail

VERSION="${1:?usage: rpm.sh <version>}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="${2:-$HERE/../target/release/prchum-gtk}"

OUTDIR="${OUTDIR:-$HERE/dist}"
mkdir -p "$OUTDIR"

BUILD="$(mktemp -d)"
trap 'rm -rf "$BUILD"' EXIT

mkdir -p "$BUILD"/{SPECS,BUILDROOT,RPMS}
ROOT="$BUILD/BUILDROOT/prchum-$VERSION"
"$HERE/stage.sh" "$ROOT" "$BINARY"

# The binary is already built and staged, so there is no %prep, %build
# or %install: the buildroot is handed over as it stands. Stripping is
# off because the binary is stripped by cargo's release profile, and
# debug package generation is off because there is nothing to put in one.
cat > "$BUILD/SPECS/prchum.spec" <<EOF
%global debug_package %{nil}
%define _build_id_links none

Name:           prchum
Version:        $VERSION
Release:        1%{?dist}
Summary:        Review pull requests and patches

License:        MIT
URL:            https://github.com/perrito666/prchum

Requires:       gtk4 >= 4.12
Requires:       libadwaita >= 1.5
Recommends:     git
Suggests:       gh

%description
Prchum reviews code: pull requests from GitHub, Forgejo and GitLab, git
comparisons, and plain patch files. Comments anchor to a semantic
location rather than to a position in a diff, so a draft survives the
branch moving under it.

It never asks for a token. Forges are reached through the command line
tools you have already authenticated, so credentials stay where you put
them.

%files
/usr/bin/prchum-gtk
/usr/bin/prchum
/usr/bin/git-prchum
/usr/share/applications/eu.dumontix.prchum.desktop
/usr/share/metainfo/eu.dumontix.prchum.metainfo.xml
/usr/share/man/man1/*
/usr/share/icons/hicolor/*/apps/eu.dumontix.prchum.png
%license /usr/share/doc/prchum/LICENSE

%changelog
* $(LC_ALL=C date '+%a %b %d %Y') Horacio Duran <horacio.duran@gmail.com> - $VERSION-1
- See the release notes at the project's homepage.
EOF

rpmbuild --define "_topdir $BUILD" \
    --buildroot "$ROOT" \
    -bb "$BUILD/SPECS/prchum.spec" > "$BUILD/rpmbuild.log" 2>&1 || {
    tail -20 "$BUILD/rpmbuild.log" >&2
    exit 1
}

OUT="$(find "$BUILD/RPMS" -name '*.rpm' | head -1)"
[[ -n "$OUT" ]] || { echo "rpmbuild produced nothing" >&2; exit 1; }
cp "$OUT" "$OUTDIR/"
echo "$OUTDIR/$(basename "$OUT")"
