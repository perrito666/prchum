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

**Status: early.** The walking skeleton is in place — the core parses
unified diffs into the canonical model and a native window renders them
(changed-files sidebar, line-number gutters, add/delete tinting). See
[`PLAN.md`](PLAN.md) for where this is going.

## Build

```sh
make run ARGS=change.diff   # build the core + app and open a patch
make check                  # what CI runs: tests, smoke test, header drift
make app                    # a double-clickable Prchum.app in dist/
```

Requires a Rust toolchain and Xcode (macOS 14+).

## License

[MIT](LICENSE)
