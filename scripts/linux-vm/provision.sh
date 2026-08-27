#!/usr/bin/env bash
# Runs inside the guest, over SSH, after cloud-init has finished.
#
# Two tiers live in this one machine: a real GNOME session for
# integration, feel, and documentation screenshots, and a headless
# compositor for the fast scripted loop. Both need the same packages,
# which is the reason for one VM rather than two.
#
# Re-running is safe.
set -euo pipefail

echo "==> GNOME desktop"
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \
    ubuntu-desktop-minimal \
    gnome-console

echo "==> GTK4 / libadwaita development packages"
# The versions Ubuntu 24.04 ships (GTK 4.14, libadwaita 1.5) satisfy what
# textchum's GTK shell already asks for, so a shell written against the
# same crates builds here without pinning anything older.
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \
    libgtk-4-dev \
    libadwaita-1-dev \
    libgtksourceview-5-dev \
    libwebkitgtk-6.0-dev \
    libssl-dev

echo "==> Testing harness"
# at-spi2-core is the accessibility bus: the analogue of the macOS
# Accessibility API that lets the harness find a button by name, click
# it, and count rows in a list. grim and weston cover the headless tier;
# gnome-screenshot covers the desktop one.
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y \
    at-spi2-core \
    libatspi2.0-dev \
    python3-pyatspi \
    weston \
    grim \
    gnome-screenshot \
    xdg-desktop-portal-gnome

echo "==> Rust toolchain"
if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustc --version

echo "==> Automatic login"
# Screenshots need a session running without someone typing a password
# into the console first.
sudo install -d /etc/gdm3
sudo tee /etc/gdm3/custom.conf >/dev/null <<'CONF'
[daemon]
AutomaticLoginEnable=true
AutomaticLogin=prchum
CONF

echo "==> Accessibility bus on for the session"
# GTK only exposes its widget tree over AT-SPI when this is set, and the
# harness is useless without it.
sudo tee /etc/environment.d/90-a11y.conf >/dev/null <<'CONF'
GTK_A11Y=atspi
CONF
gsettings set org.gnome.desktop.interface toolkit-accessibility true || true

echo "==> Silencing the first-run wizard"
# Ubuntu's welcome tour sits on top of everything, which is no way to
# photograph an application.
sudo mkdir -p /home/prchum/.config
echo "yes" | sudo tee /home/prchum/.config/gnome-initial-setup-done >/dev/null
sudo chown -R prchum:prchum /home/prchum/.config

echo "==> Graphical target"
sudo systemctl set-default graphical.target

echo
echo "Provisioned. Reboot for the GNOME session to come up logged in:"
echo "    sudo reboot"
