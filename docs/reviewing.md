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
| ⇧⌥⌘↓ / ⇧⌥⌘↑ | next / previous file not yet reviewed |
| ⌥⌘V | mark the current file as reviewed ↔ unmark it |
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

## Reviewed files

⌥⌘V marks the file on screen as reviewed; the sidebar puts a check in
front of it, dims its name, and counts progress at the top — "3 of 12
reviewed". Clicking the check does the same. ⇧⌥⌘↓ goes to the next file
not yet reviewed, wrapping around the end of the list, and ⇧⌥⌘↑ to the
previous one; when every file is marked, it says so.

A mark belongs to the changes it was made against. If a file's diff
changes — a new push, an edited working tree — its mark lapses and the
file counts as unreviewed again, the way GitHub's "Viewed" does. A
rebase that only moves line numbers keeps the mark.

Marks are yours alone: they are saved with your drafts and never sent to
the forge. An exchange document does not carry them either. The Linux
shell does not have marks yet.

## One commit at a time

A pull request's window has a commit picker at the start of its
toolbar: **All changes**, then every commit of the request, oldest
first, as `abc1234  title`. Picking a commit replaces the review in the
same window with that commit's changes against its parent; picking
**All changes** goes back. The title shows `owner/repo#N @ abc1234`
while you are on a commit. (On macOS for now; the Linux shell does not
have the picker yet.)

| Default | Action |
| --- | --- |
| ⌃⌘C | review commit: the same list as a menu, arrows and Return |
| ⌃⌘↓ / ⌃⌘↑ | next / previous commit (All changes comes first) |

Each commit is a review of its own. Its drafts are kept apart from the
whole request's and from the other commits', and come back when you
pick that commit again; the picker counts the drafts waiting on each.

Comments written on a commit are submitted to the pull request **pinned
to that commit**, the way the forge's own single-commit view posts them
(see [Forges](forges.md)). If the forge refuses one, the error is shown
and the drafts stay for a retry. Existing review threads are not drawn
inline on a commit — their positions belong to the whole request's diff
— but the conversation is there as always.

GitHub lists at most 250 commits of a pull request. On a longer one the
picker says so under the list; the later commits can still be reviewed
in **All changes**.

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
| ⌥↩ | open the conversation at the caret in its own reader |
| ⇧⌘T | expand ↔ collapse the resolved thread at the caret |
| ⌃⌘E | edit the current file locally (see below) |

A selection must map onto one side, GitHub-style: a changed block
anchors RIGHT (the deletions are simply not part of that side), a
deletions-only selection anchors LEFT. Drafts show as `●` in the gutter
with the note inline; existing host threads show as `◆`.

### Outdated and resolved threads

A thread is **outdated** when the code it was written against has since
changed, so it no longer sits on any line of the diff. Rather than
drawing it beside whatever code now has that line number, prchum lists
outdated threads at the top of their file, one line each — author,
the line it was on, the start of the comment. **Read…** (or ⌥↩ on
that line) opens the whole conversation in the reader, where you can
still reply.

A **resolved** thread that is still on a line collapses to one line
under it. **Expand**, or ⇧⌘T with the caret on it, shows it in full;
**Collapse** or ⇧⌘T again folds it back. ⌥↩ opens it in the reader
without expanding it.

The review navigator (⌘L) lists both, labelled *outdated* or
*resolved*; Return jumps to a thread that has a line and opens one
that does not. The sidebar's `◆` count includes them.

Resolving and reopening threads is left to the forge. GitHub reports
resolution only through its GraphQL API; if that query fails, threads
show in full rather than keeping the review from opening.

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
