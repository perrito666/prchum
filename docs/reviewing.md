# Reviewing

Every operation is a named action: a menu item with a macOS-native key
equivalent, rebindable through the `keys` map in
[configuration](configuration.md). The mouse works everywhere, but
nothing requires it.

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

A selection must map onto one side, GitHub-style: a changed block
anchors RIGHT (the deletions are simply not part of that side), a
deletions-only selection anchors LEFT. Drafts show as `●` in the gutter
with the note inline; existing host threads show as `◆`.

Dismissed is not deleted: the verdict travels with the review — it is
the information the other side of a conversation needs most — but a
dismissed comment is never submitted.

## Submitting

⇧⌘↩ opens the submit sheet on a pull-request session: the counts, an
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
