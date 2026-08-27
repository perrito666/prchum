# Getting started

## Install

Grab `Prchum.app` from the
[releases](https://github.com/perrito666/prchum/releases) — signed and
notarized, so download, unzip and double-click; Apple Silicon, macOS
14+. On Linux, take the `prchum-gtk` tarball from the same place; it
needs GTK 4.12 and libadwaita 1.5 or newer. Or build from source:

```sh
make run ARGS=change.diff   # build the core + app and open a patch
make app                    # a double-clickable Prchum.app in dist/
make check                  # what CI runs: tests, smoke test, header drift
```

Building needs a Rust toolchain and Xcode. Reviewing pull requests needs
the forge's CLI installed and authenticated: [`gh`](https://cli.github.com)
(`gh auth login`) for GitHub, [`fj`](https://codeberg.org/forgejo-contrib/forgejo-cli)
for Forgejo. Patch-file and local-git review need nothing but `git`.

## First review

Launching with no target asks what to review: a pull request, your
review queue, a patch file, or a git repository. Every door is also in
the File menu, and everything works from the command line:

```sh
Prchum change.diff                           # a patch file
Prchum 418                                   # PR 418 of the current repo's origin
Prchum owner/repo#418                        # explicit repository
Prchum https://github.com/owner/repo/pull/418
```

Once the diff is open:

1. **Navigate.** ⌘↓/⌘↑ jump between changes, ⌥⌘↓/⌥⌘↑ between hunks,
   ⇧⌘↓/⇧⌘↑ between files — or click a file in the sidebar.
2. **Comment.** Place the caret (or select lines) and press ⌘↩. Write
   the note, press Comment. The line gets a `●` marker and the note
   shows inline under it.
3. **Submit or export.** On a pull request, ⇧⌘↩ opens the submit sheet
   — nothing is sent before you confirm. Anywhere, ⇧⌘E exports your
   notes as Markdown (or a `.json` review-exchange document).

Drafts persist automatically per source and reload the next time you
open the same comparison.
