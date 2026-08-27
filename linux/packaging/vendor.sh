#!/usr/bin/env bash
# Vendors the GTK shell's dependencies into linux/vendor/, which the
# Flatpak build consumes offline.
#
#   ./vendor.sh
#
# Run it whenever linux/Cargo.lock changes. The directory is ignored by
# git — it is a build input, reproducible from the lockfile, not
# something to review.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE/.."

cargo vendor --locked vendor > /dev/null
echo "vendored $(find vendor -maxdepth 1 -mindepth 1 -type d | wc -l | tr -d ' ') crates into linux/vendor"
