# A guided tour

A walk through a review from start to finish, with the windows you will
actually see. The screenshots follow this page: light ones in light
mode, dark ones in dark. They use the `default` syntax theme, the
repository under review is a small chess move generator, and the
reviewer is `ada`.

## The home screen

Prchum opens on the home screen: four ways in across the top, and
underneath, the reviews you have opened before.

![The home screen](images/home-light.png#only-light)
![The home screen](images/home-dark.png#only-dark)

The history remembers where a review came from and whether you submitted
it. Rows for requests that have since merged or closed are pruned, and
so are the worktrees prchum made for them.

## The review queue

⇧⌘L asks your forge which requests are waiting for you.

![The review queue](images/review-queue-light.png#only-light)
![The review queue](images/review-queue-dark.png#only-dark)

The picker at the top chooses the filter. Named filters come from the
`list_filters` map in your configuration, the default one runs when you
pick nothing, and **Custom…** takes a filter typed on the spot for the
rest of the session.

![The filter picker](images/queue-filters-light.png#only-light)
![The filter picker](images/queue-filters-dark.png#only-dark)

Return, or a double click, opens the highlighted request.

## Reading a diff

The review window is the sidebar of changed files, the toolbar, and the
diff.

![A review window](images/review-window-light.png#only-light)
![A review window](images/review-window-dark.png#only-dark)

The sidebar counts additions and deletions per file, and marks files
that carry comments. ⌘↓ and ⌘↑ step through changes, ⌥⌘↓ and ⌥⌘↑ through
hunks, ⇧⌘↓ and ⇧⌘↑ through files. Syntax coloring runs one tree-sitter
pass per side of each hunk, so a construct that spans several lines
colors correctly on both the old and the new text.

Opening a pull request looks the same — the title bar names the request
instead of the comparison.

![A pull request](images/pull-request-light.png#only-light)
![A pull request](images/pull-request-dark.png#only-dark)

⌘I shows the request's description, rendered as Markdown, with the
branch it merges into and a button to open it in a browser.

![Pull request info](images/pr-info-light.png#only-light)
![Pull request info](images/pr-info-dark.png#only-dark)

## Split view

⌥⌘T puts the two sides in parallel panels. The panel your caret sits in
decides which side a new comment targets.

![Split view](images/split-view-light.png#only-light)
![Split view](images/split-view-dark.png#only-dark)

## Full-file context

A diff shows three lines around each change, which is often three lines
too few.

![The hunks alone](images/hunk-view-light.png#only-light)
![The hunks alone](images/hunk-view-dark.png#only-dark)

⌥⌘C fetches the whole file and lays the hunks back into it, so you read
the change where it lives. The code outside the diff is colored too, and
the fetch happens off the main thread — the window stays live while it
arrives.

![Full-file context](images/context-view-light.png#only-light)
![Full-file context](images/context-view-dark.png#only-dark)

## Commenting

⌘↩ comments on the line under the caret, or on the selection.

![The comment composer](images/comment-composer-light.png#only-light)
![The comment composer](images/comment-composer-dark.png#only-dark)

A selection spanning several lines becomes a range comment, the way the
forge understands it.

![A multi-line comment](images/multiline-comment-light.png#only-light)
![A multi-line comment](images/multiline-comment-dark.png#only-dark)

Comments are not attached to a row on screen. They anchor to a semantic
location — file, side, line range, and a short context anchor with a
hash of the line's content — which is why a draft survives the branch
moving under it.

Drafts and the threads already on the request appear inline, framed,
with their Markdown rendered.

![A comment thread](images/comment-light.png#only-light)
![A comment thread](images/comment-dark.png#only-dark)

⌘R replies into a thread, ⌘E edits the draft under the caret, ⇧⌘X
dismisses one (kept locally, never submitted), and ⌘⌫ deletes it.

## The review navigator

⌘L lists every draft and thread in the review; Return jumps to the one
you pick.

![The review navigator](images/navigator-light.png#only-light)
![The review navigator](images/navigator-dark.png#only-dark)

## Submitting

⇧⌘↩ opens the submit sheet: how many comments and replies are about to
go out, a box for the review summary, and the event — comment, approve,
or request changes. ⌥⌘A and ⌥⌘R open the same sheet with approve or
request-changes already chosen.

Comments the forge accepts are dropped from your local drafts as they
land, so a submission that fails halfway can be retried without posting
anything twice. Orphaned drafts — the ones whose code is gone — are
never submitted, and the sheet says so before you commit to it.

## Settings

⌘, holds the things worth changing.

![Settings](images/settings-light.png#only-light)
![Settings](images/settings-dark.png#only-dark)

Appearance and theme; the name your drafts are attributed to; the
default and named discovery filters; the editor template; and the map
from forge repositories to local clones that **Edit File Locally**
(⌃⌘E) uses to check the branch out and open the file where your caret
is.

Everything here writes through to `config.json`, which stays
hand-editable: unknown keys survive every save, and a file prchum
cannot parse is never overwritten.

## On Linux

The same review, in the GTK shell. Prchum's core is one portable
library; what changes between platforms is the presentation, and it
changes deliberately — this is a GNOME application, not a Mac one
wearing a different theme.

![The review window on Linux](images/linux-review-light.png#only-light)
![The review window on Linux](images/linux-review-dark.png#only-dark)

The rows are identical because the core decides them: the same files,
the same markers and line numbers, the same tree-sitter colours from the
same style table. What differs is everything around them — a libadwaita
header bar carrying the title and the file, GNOME's own window controls,
and its accent colours in the sidebar's counts. It follows the desktop's
light and dark setting, as the picture above does.

Commenting works as it does on macOS, because it is the same core doing
it: **Ctrl+Return** opens the composer on the line under the cursor,
**Ctrl+E** edits a draft, **Ctrl+Delete** removes one, and
**Ctrl+Shift+X** dismisses it. Drafts appear inline under the line they
belong to and survive closing the window — they are written beside the
configuration, under `~/.local/share/prchum`, following the XDG layout
rather than the macOS one.

The chords differ, and on purpose. Actions have the same names on both
platforms and the same entries in the `keys` map, but a GNOME user
presses Ctrl where a Mac user presses Command, so the defaults are
Ctrl-shaped: **Ctrl+↑/↓** steps through changes, **Ctrl+Shift+↑/↓**
through files.

The rest is here too. **Ctrl+Alt+C** lays the hunks back into the whole
file; **Ctrl+Shift+T** puts the two sides in parallel panels that scroll
together; **Ctrl+Shift+L** asks the forge what is waiting and opens what
you pick; **Ctrl+,** holds the settings, written to the same
`config.json` the macOS app writes; and **Ctrl+Shift+Return** submits,
with the same retry-safety — whatever the forge accepted leaves your
drafts even if a later step fails.

!!! note "What is not there yet"

    Two things the macOS app has and this does not: replying into a
    thread from the diff, and the conversation screen for comments that
    belong to the request rather than to a line. Threads themselves are
    shown; it is answering them in place that is missing.
