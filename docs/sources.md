# Sources

Prchum reviews four kinds of source in the same window.

## Patch files

Open a `.diff`/`.patch` file (File → Open…, drag onto the app, or the
command line). Drafts key on the file's absolute path, so reviewing the
same file again resumes the same notes.

## Local git comparisons

File → Review Git Repository… picks a checkout and a comparison:

- **Working tree vs HEAD** — what `git diff` shows.
- **Staged (index) vs HEAD** — what `git diff --cached` shows.
- **Against a base ref** — `base...HEAD`, the merge-base comparison.

Each comparison keeps its own draft, so notes on the staged changes
never mix with notes against `main`.

## Pull requests

A URL, `owner/repo#N`, or a bare number (repository inferred from the
current directory's origin). Prchum fetches the host's **canonical
diff**, so comment positions always match what the forge shows, plus the
existing review threads (`◆` markers — ⌘R replies) and the pull
request's metadata (⌘I). See [Forges](forges.md) for hosts and
authentication.

If the head moves between sessions, saved drafts re-anchor by their
captured context: an exact match keeps its place, a unique text match
follows the code, and anything ambiguous is **orphaned** — kept, marked,
and never submitted. Guessing would put a note on the wrong line.

## The review-exchange format

Prchum reads and writes leanreview's `*.review.json` format (version 1),
detected by content, never by filename. An LLM writes its review into a
self-contained document; you triage it in Prchum — dismiss, edit, reply
— and every change rewrites the file in place, so quitting leaves the
conversation current for the model's next round. Unchanged documents
round-trip byte-identically, so the two clients interoperate on the same
files.
