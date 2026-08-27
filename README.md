# Prchum

[![Documentation](https://img.shields.io/badge/docs-perrito666.github.io%2Fprchum-f59e0b)](https://perrito666.github.io/prchum/)
[![Latest release](https://img.shields.io/github/v/release/perrito666/prchum)](https://github.com/perrito666/prchum/releases/latest)
[![CI](https://github.com/perrito666/prchum/actions/workflows/ci.yml/badge.svg)](https://github.com/perrito666/prchum/actions/workflows/ci.yml)

A native macOS code-review client: [leanreview](https://github.com/perrito666/leanreview)'s
functionality on [textchum](https://github.com/perrito666/textchum)'s
architecture. Review a **patch/diff file**, a **local git comparison**, or
a **pull/merge request** on **GitHub**, **Forgejo**, or **GitLab** in the
same app:
navigate the diff, attach draft comments anchored to semantic diff
locations, then export them as Markdown / review-exchange JSON or submit
them as a real review.

Prchum is built as a portable compiled core (Rust) behind a fully native
shell (Swift + AppKit), meeting at a C interface. The core owns the diff
and the review state; the shell owns the platform. It is a review client,
not a git client: the installed `git`, `gh`, `fj`, and `glab` handle
repository and forge semantics.

Full documentation — workflows, sources, forges, and configuration, in
English, Spanish, and French — lives at
**[perrito666.github.io/prchum](https://perrito666.github.io/prchum/)**.

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

Also in: tree-sitter syntax highlighting on both sides of a change
(cmd+alt+s cycles the mode), split view (cmd+alt+t), hunk folding
(cmd+alt+left), and the review queue (File → My Review Queue) listing
the requests waiting on you via gh or Forgejo; suggestion fences
(cmd+alt+return), the review navigator (cmd+l), the full-file context
view (cmd+alt+c), the conversation screen (cmd+shift+p), themes and
appearance in Settings (cmd+comma), inline images in comment views, the
home screen with your review history (pruned when requests merge or
close), and the `pr` command-line tool (`make install-cli`) driving the
app through its prchum:// scheme. See [`PLAN.md`](PLAN.md).

## Build

```sh
make run ARGS=change.diff   # build the core + app and open a patch
make check                  # what CI runs: tests, smoke test, header drift
make app                    # a double-clickable Prchum.app in dist/
```

Requires a Rust toolchain and Xcode (macOS 14+).

Releases build from `v*` tags, and the macOS app is signed and
notarized: the workflow does it by itself from the Apple secrets in the
repository, which `scripts/setup-signing` loads (certificate, password,
and notary auth) via `gh secret set`. Without those secrets it still
builds, and says in the release notes that the app is unsigned. Linux
releases carry a `prchum-gtk` tarball alongside.

## The icon

The app icon is a sunflower photographed by Horacio Duran in a field
near Melnik on 6 July 2026 at 15:05.

## License

[MIT](LICENSE)
