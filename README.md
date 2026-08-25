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
(worktree/staged/base/range), GitHub and Forgejo PR, and review-exchange
sessions;
draft comments anchored to semantic locations with conservative
relocation when the head moves; inline previews (`●` drafts, `◆` host
threads), replies, dismiss-not-delete; per-source draft persistence and
exchange writeback; Markdown/exchange export; and submission as one
atomic review with retry-safe accounting. Keyboard-first via menus with
rebindable keys (`keys` in config.json); mouse works but is secondary.
**Forgejo**: PRs on Codeberg or a self-hosted instance work through the
[`fj` CLI](https://codeberg.org/forgejo-contrib/forgejo-cli) (`fj -H
<host> auth …` once, like `gh auth login`). A host that doesn't say what
it is in its name gets declared in `~/Library/Application
Support/Prchum/config.json`:

```json
{
  "forges": { "git.example.com": "forgejo" },
  "forgejo_api_command": "fj -H {host} api {method} {path}"
}
```

`forgejo_api_command` is optional — the default above targets `fj` — and
exists so the transport can follow whatever CLI your instance
standardizes on (the JSON body arrives on stdin; prchum never stores a
token itself).

Not yet: syntax highlighting, split view, folding, context view, themes,
GitLab, discovery. See [`PLAN.md`](PLAN.md).

## Build

```sh
make run ARGS=change.diff   # build the core + app and open a patch
make check                  # what CI runs: tests, smoke test, header drift
make app                    # a double-clickable Prchum.app in dist/
```

Requires a Rust toolchain and Xcode (macOS 14+).

## The icon

The app icon is a sunflower photographed by Horacio Duran in a field
near Melnik on 6 July 2026 at 15:05.

## License

[MIT](LICENSE)
