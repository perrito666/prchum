#!/usr/bin/env bash
# Photograph the guest's screen.
#
#   ./vm-shot.sh out.png
#
# The picture is taken on the host, of UTM's display window, rather than
# inside the guest. That is not a shortcut: GNOME refuses programmatic
# screenshots. Its Shell.Screenshot D-Bus method answers "Screenshot is
# not allowed" to anything that is not an interactive user action, and
# grim does not work under Mutter, which implements none of the wlroots
# screencopy protocol. Capturing the window UTM already draws sidesteps
# the argument entirely, and it is the same `screencapture -l` the macOS
# side of the harness uses — the two platforms end up photographed the
# same way.
set -euo pipefail

VM_NAME="${VM_NAME:-prchum-linux}"
OUT="${1:-}"
[[ -n "$OUT" ]] || { echo "usage: $(basename "$0") <out.png>" >&2; exit 2; }

WID="$(swift - "$VM_NAME" <<'SWIFT'
import CoreGraphics
import Foundation
let wanted = CommandLine.arguments[1]
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID)
    as! [[String: Any]]
for window in windows where (window["kCGWindowName"] as? String) == wanted {
    print(window["kCGWindowNumber"] as! Int)
    break
}
SWIFT
)"

[[ -n "$WID" ]] || {
    echo "no window named $VM_NAME — is the machine started and its display open?" >&2
    exit 1
}

screencapture -x -o -l "$WID" "$OUT"
echo "$OUT"
