# The Linux development VM

Prchum's Linux shell is a GTK4 and libadwaita app, and the thing it has
to be good at is being a GNOME application. That cannot be judged from a
headless container: the parts most likely to be wrong — portals for the
file chooser and for opening an editor, the `prchum://` scheme handler,
libadwaita following the system colour scheme, accelerators GNOME
reserves for itself, client-side decorations, fractional scaling — only
exist inside a real session.

So: one virtual machine, holding two tiers.

* A **GNOME session**, logged in automatically, for integration, for
  judging how the app feels, and for documentation screenshots.
* A **headless compositor** (`weston --backend=headless`) in the same
  guest for the fast scripted loop. It renders real libadwaita widgets
  into a framebuffer that `grim` can capture, which is enough to catch
  layout faults — a list with rows in the accessibility tree and nothing
  painted, say — without waiting for a desktop.

The second tier is also what a GitHub `ubuntu-latest` job can run, so
the quick loop stays reproducible away from this machine.

## Building it

Needs UTM and `qemu-img` (`brew install qemu`) — UTM keeps its own copy
of qemu-img as a library it loads in-process, which the shell cannot
call.

```sh
./create-vm.sh
```

Downloads the Ubuntu 24.04 LTS arm64 cloud image, grows a disk from it,
writes a cloud-init seed carrying your public key, and creates and
starts the machine through UTM's scripting interface. It prints the
guest's address and the two commands that provision it.

The VM is disposable by design. When it drifts, throw it away and build
another:

```sh
./create-vm.sh --delete
```

Everything downloaded or generated lands in `build/`, which is ignored.
Nothing identifying is committed: `user-data.template` carries a
placeholder, and your public key is read at build time.

## Working with it

```sh
./vm-ssh.sh                     a shell in the guest
./vm-ssh.sh 'cargo build'       one command
./vm-shot.sh shot.png           a picture of its screen
./vm-exec.sh 'systemctl status' when the network is not up yet
```

`vm-ssh.sh` looks the address up every time, and uses a key generated
for this machine alone. Both of those are deliberate.

The address changes: the guest takes a fresh DHCP lease when it reboots,
and a remembered address then fails as **no route to host** — which
reads like a broken network rather than a stale number. It cost an
afternoon here, so nothing in this directory hardcodes it.

The key is the machine's own, without a passphrase, kept in `build/`.
A personal key usually has a passphrase and needs an agent that scripts
do not have, and a disposable VM has no business holding one.

`vm-exec.sh` goes over the guest agent's virtio-serial channel instead
of the network, which is what you want before the guest has an address,
or when it has stopped answering and you need to find out why.

## Screenshots, and why the session runs on Xorg

Under Wayland the session cannot be photographed or driven. GNOME's
`Shell.Screenshot` answers *"Screenshot is not allowed"* to anything
that is not an interactive user action, and `grim` does nothing under
Mutter, which implements none of the wlroots screencopy protocol.

So `provision.sh` sets `WaylandEnable=false` and the session runs on
Xorg, where `import -window <id>` captures one window cleanly and
`xdotool` can type and click. `vm-shot.sh` takes a window title and
fits the window on screen first, because X11 reads a window's pixels
out of the screen and anything hanging off the edge comes back with the
desktop behind it.

That matters for the manual: the macOS images are single windows, and
these need to match. `--host` falls back to capturing UTM's window from
the outside, for when you have deliberately switched to Wayland to check
fractional scaling or other Wayland-specific behaviour.

## Driving the app

The harness mirrors the macOS one. There, the shell is driven through
the Accessibility API; here, GTK publishes the same kind of widget tree
over **AT-SPI**, so a script can find a button by its label, click it,
and read a list's row count.

That correspondence is the point. The bug that prompted this — a
settings table holding rows that were never painted — was found by
asking the accessibility tree what it contained and comparing that with
the pixels. The same question can be asked on both platforms.
