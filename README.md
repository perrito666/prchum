# Prchum

A native macOS code-review client: [leanreview](https://github.com/perrito666/leanreview)'s
functionality on [textchum](https://github.com/perrito666/textchum)'s
architecture. Review a **patch/diff file**, a **local git comparison**, a
**GitHub pull request**, or a **GitLab merge request** in the same app:
navigate the diff, attach draft comments anchored to semantic diff
locations, then export them as Markdown / review-exchange JSON or submit
them as a real review.

Prchum is built as a portable compiled core (Rust) behind a fully native
shell (Swift + AppKit), meeting at a C interface. The core owns the diff
and the review state; the shell owns the platform. It is a review client,
not a git client: the installed `git`, `gh`, and `glab` handle repository
and forge semantics.

**Status: early but reviewing.** Working today: patch-file, git
(worktree/staged/base/range), GitHub PR, and review-exchange sessions;
draft comments anchored to semantic locations with conservative
relocation when the head moves; inline previews (`●` drafts, `◆` host
threads), replies, dismiss-not-delete; per-source draft persistence and
exchange writeback; Markdown/exchange export; and submission as one
atomic review with retry-safe accounting. Keyboard-first via menus with
rebindable keys (`keys` in config.json); mouse works but is secondary.
Not yet: syntax highlighting, split view, folding, context view, themes,
GitLab, discovery. See [`PLAN.md`](PLAN.md).

## Build

```sh
make run ARGS=change.diff   # build the core + app and open a patch
make check                  # what CI runs: tests, smoke test, header drift
make app                    # a double-clickable Prchum.app in dist/
```

Requires a Rust toolchain and Xcode (macOS 14+).

## License

[MIT](LICENSE)
