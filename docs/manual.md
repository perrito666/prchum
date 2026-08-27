# A guided tour

A walk through a review from start to finish, with the windows you will
actually see. The screenshots use the dark appearance and the `default`
theme; the repository under review is a small chess move generator, and
the reviewer is `ada`.

## The home screen

Prchum opens on the home screen: four ways in across the top, and
underneath, the reviews you have opened before.

![The home screen](images/home.png)

The history remembers where a review came from and whether you submitted
it. Rows for requests that have since merged or closed are pruned, and
so are the worktrees prchum made for them.

## The review queue

⇧⌘L asks your forge which requests are waiting for you.

![The review queue](images/review-queue.png)

The picker at the top chooses the filter. Named filters come from the
`list_filters` map in your configuration, the default one runs when you
pick nothing, and **Custom…** takes a filter typed on the spot for the
rest of the session.

![The filter picker](images/queue-filters.png)

Return, or a double click, opens the highlighted request.

## Reading a diff

The review window is the sidebar of changed files, the toolbar, and the
diff.

![A review window](images/review-window.png)

The sidebar counts additions and deletions per file, and marks files
that carry comments. ⌘↓ and ⌘↑ step through changes, ⌥⌘↓ and ⌥⌘↑ through
hunks, ⇧⌘↓ and ⇧⌘↑ through files. Syntax coloring runs one tree-sitter
pass per side of each hunk, so a construct that spans several lines
colors correctly on both the old and the new text.

Opening a pull request looks the same — the title bar names the request
instead of the comparison.

![A pull request](images/pull-request.png)

⌘I shows the request's description, rendered as Markdown, with the
branch it merges into and a button to open it in a browser.

![Pull request info](images/pr-info.png)

## Split view

⌥⌘T puts the two sides in parallel panels. The panel your caret sits in
decides which side a new comment targets.

![Split view](images/split-view.png)

## Full-file context

A diff shows three lines around each change, which is often three lines
too few.

![The hunks alone](images/hunk-view.png)

⌥⌘C fetches the whole file and lays the hunks back into it, so you read
the change where it lives. The code outside the diff is colored too, and
the fetch happens off the main thread — the window stays live while it
arrives.

![Full-file context](images/context-view.png)

## Commenting

⌘↩ comments on the line under the caret, or on the selection.

![The comment composer](images/comment-composer.png)

A selection spanning several lines becomes a range comment, the way the
forge understands it.

![A multi-line comment](images/multiline-comment.png)

Comments are not attached to a row on screen. They anchor to a semantic
location — file, side, line range, and a short context anchor with a
hash of the line's content — which is why a draft survives the branch
moving under it.

Drafts and the threads already on the request appear inline, framed,
with their Markdown rendered.

![A comment thread](images/comment.png)

⌘R replies into a thread, ⌘E edits the draft under the caret, ⇧⌘X
dismisses one (kept locally, never submitted), and ⌘⌫ deletes it.

## The review navigator

⌘L lists every draft and thread in the review; Return jumps to the one
you pick.

![The review navigator](images/navigator.png)

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

![Settings](images/settings.png)

Appearance and theme; the name your drafts are attributed to; the
default and named discovery filters; the editor template; and the map
from forge repositories to local clones that **Edit File Locally**
(⌃⌘E) uses to check the branch out and open the file where your caret
is.

Everything here writes through to `config.json`, which stays
hand-editable: unknown keys survive every save, and a file prchum
cannot parse is never overwritten.
