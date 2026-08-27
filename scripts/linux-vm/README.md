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

Commands go over the guest agent's virtio-serial channel, not the
network:

```sh
./vm-exec.sh 'cargo build'
./vm-exec.sh 'tail -20 provision.log'
```

Screenshots are taken on the host, of the window UTM already draws:

```sh
./vm-shot.sh shot.png
```

`ssh prchum@<address>` is nicer for long sessions and does work — but
only once macOS has been told to allow it. If ssh says "no route to
host" while the guest's own network is fine, the guest is not the
problem: macOS withholds local network access from the terminal until
you grant it under System Settings > Privacy & Security > Local Network.
Nothing in this directory depends on it.

## Why screenshots come from the host

Because GNOME will not take one for you. `Shell.Screenshot` over D-Bus
answers *"Screenshot is not allowed"* to anything that is not an
interactive user action, and `grim` is useless here because Mutter
implements none of the wlroots screencopy protocol it relies on.

Capturing UTM's window sidesteps all of that, and it lands somewhere
better than a workaround: it is the same `screencapture -l` used to
photograph the macOS app, so both platforms are photographed the same
way, at the same retina scale.

## Driving the app

The harness mirrors the macOS one. There, the shell is driven through
the Accessibility API; here, GTK publishes the same kind of widget tree
over **AT-SPI**, so a script can find a button by its label, click it,
and read a list's row count.

That correspondence is the point. The bug that prompted this — a
settings table holding rows that were never painted — was found by
asking the accessibility tree what it contained and comparing that with
the pixels. The same question can be asked on both platforms.
