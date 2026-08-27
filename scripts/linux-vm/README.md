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

## Driving the app inside it

The harness mirrors the macOS one. There, the shell is driven through
the Accessibility API and photographed with `screencapture`; here, GTK
publishes the same kind of widget tree over **AT-SPI**, so a script can
find a button by its label, click it, read a list's row count, and then
capture the window with `grim` or `gnome-screenshot`.

That correspondence is the point. The bug that prompted this — a
settings table holding rows that were never painted — was found by
asking the accessibility tree what it contained and comparing that with
the pixels. The same question can be asked on both platforms.
