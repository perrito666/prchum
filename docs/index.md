# Prchum

A native macOS code-review client. Review a **patch file**, a **local git
comparison**, a **GitHub pull request**, or a **Forgejo pull request** in
the same app: navigate the diff, attach draft comments anchored to
semantic locations, then export your notes as Markdown or a
review-exchange document — or submit them as a real review.

Prchum is [leanreview](https://github.com/perrito666/leanreview)'s
functionality rebuilt on
[textchum](https://github.com/perrito666/textchum)'s architecture: a
portable compiled core (Rust) behind a fully native shell (Swift +
AppKit), meeting at a C interface. The core owns the diff and the review
state; the shell owns the platform.

It is a review client, not a git client: the installed `git`, `gh`, and
`fj` handle repository and forge semantics — Prchum never stores a
token. It owns navigation, rendering, review state, and comments.

## The one rule everything follows

Rendered rows are never canonical comment locations. Every comment
anchors to a semantic **location** — path, side (LEFT/RIGHT), line
range, and the text around it — so it survives layout changes, folding,
unified↔split toggles, and, through conservative relocation, even the
pull request's head moving between sessions.

## Where to go next

- [Getting started](getting-started.md) — install, build, first review.
- [Reviewing](reviewing.md) — navigation, comments, submission.
- [Sources](sources.md) — patches, git, pull requests, the exchange
  format.
- [Forges](forges.md) — GitHub, Forgejo, and self-hosted instances.
- [Configuration](configuration.md) — keys, forges, discovery.
