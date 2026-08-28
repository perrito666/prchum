# Reviewing

Every operation is a named action: a menu item with a macOS-native key
equivalent, rebindable through the `keys` map in
[configuration](configuration.md). The mouse works everywhere, but
nothing requires it. The window's toolbar carries the common ones for
mouse days, and ⇧⌘H returns to the home screen; sheets confirm with ⌘↩
(Return types a newline in the body).

## Navigation and view

| Default | Action |
| --- | --- |
| ⌘↓ / ⌘↑ | next / previous change |
| ⌥⌘↓ / ⌥⌘↑ | next / previous hunk |
| ⇧⌘↓ / ⇧⌘↑ | next / previous file |
| ⌘F | find in the diff (the native find bar) |
| ⌥⌘T | unified ↔ split view |
| ⌥⌘C | full-file context: the whole file with the hunks overlaid |
| ⌥⌘← | fold / unfold the current hunk |
| ⇧⌥⌘← / ⇧⌥⌘→ | collapse / expand all hunks |
| ⌥⌘S | cycle syntax coloring: syntax + tints → tints only → plain |
| ⌥⌘W | wrap long lines |
| ⌃⌘S | toggle the changed-files sidebar |

Syntax highlighting runs one tree-sitter pass per side per hunk, so
multi-line constructs color correctly on both the old and the new side
of a change. Fourteen languages ship built in.

In split view the two sides sit in parallel panels; the panel your caret
is in decides which side a comment targets.

## Comments

| Default | Action |
| --- | --- |
| ⌘↩ | comment on the caret's line or the selection |
| ⌘E | edit the draft under the caret |
| ⌘⌫ | delete the draft under the caret |
| ⇧⌘X | dismiss ↔ restore (kept, never submitted while dismissed) |
| ⌥⌘↩ | suggest a change: the selection's code prefilled in a ```suggestion fence |
| ⌘R | reply — to the host thread or the draft conversation at the caret |
| ⌘L | the review navigator: every draft and thread, Return jumps |
| ⌃⌘E | edit the current file locally (see below) |

A selection must map onto one side, GitHub-style: a changed block
anchors RIGHT (the deletions are simply not part of that side), a
deletions-only selection anchors LEFT. Drafts show as `●` in the gutter
with the note inline; existing host threads show as `◆`.

Dismissed is not deleted: the verdict travels with the review — it is
the information the other side of a conversation needs most — but a
dismissed comment is never submitted.

## Editing locally

⌃⌘E opens the file under the caret in your editor, in a local checkout
of the branch under review — at the caret's line when that line exists
in the file (a deletion opens the file without one).

The checkout comes from the clone you point at in
[configuration](configuration.md): if the branch is already checked out
there — in the clone itself or a worktree you made — that one is used and
left alone; otherwise prchum creates a worktree of its own beside its
state, fetching the request's head when the branch is not local yet.
Only the worktrees prchum created are ever removed, and only when the
request has merged, closed, or vanished.

A git comparison needs no clone: it already is a checkout, so the file
opens right there.

## Submitting

⇧⌘↩ opens the submit sheet on a pull-request session; ⌥⌘A opens it
with **Approve** preselected and ⌥⌘R with **Request changes** — the
sheet still confirms either way. It shows the counts, the
event picker (Comment / Approve / Request changes), the summary, and an
explicit warning for orphaned comments, which are never submitted.
Nothing is sent before this confirmation.

Submission is retry-safe: the app records exactly what the host
accepted, even when a later step fails, so a retry sends only what is
still pending — never a duplicate.

## Exporting

⇧⌘E writes your notes to a file: Markdown grouped by file, or — with a
`.json` name — a self-contained review-exchange document (see
[Sources](sources.md)) embedding the patch.

## From the terminal

Prchum is meant to be reached for the way `git diff` is, so it takes the
same shapes of argument:

```sh
prchum                  # what git diff would show
prchum --staged         # what git diff --staged would show
prchum main             # this branch against main
prchum v1..v2           # a range
prchum change.diff      # a patch or exchange file
prchum 418              # request 418 of this repository's origin
prchum owner/repo#418   # an explicit repository
```

`git prchum` does the same, because git treats any `git-*` on the PATH
as a subcommand — and it runs from the repository's top level, so it
means the same thing in a subdirectory as it does at the root.

To make it the command you reach for, give git an alias:

```sh
git config --global alias.d '!git prchum'
```

Then `git d` opens what `git diff` would have printed, and `git d main`
compares against a branch.

Both commands have man pages: `man prchum`, `man git-prchum`, and
`git prchum --help`, which git answers from the same page.

On macOS the command is installed from **Prchum → Install Command-Line
Tool…**, or with `make install-cli` from a checkout. On Linux the
packages install it.

!!! note "It used to be called `pr`"

    Which was a mistake: `pr` is POSIX's paginator, it has a man page,
    and `/usr/local/bin` comes before `/usr/bin` on the default PATH — so
    installing it there quietly shadowed a standard tool. If you have the
    old one, `rm /usr/local/bin/pr` puts things back.
