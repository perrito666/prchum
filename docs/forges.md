# Forges

Prchum talks to forges through their command-line clients, never with
its own credentials: authentication, token storage, and enterprise hosts
stay the CLI's already-solved problem.

## GitHub

Install [`gh`](https://cli.github.com) and run `gh auth login` once.
Everything rides `gh api`: metadata, the canonical diff, review threads,
and submission — one atomic review carrying every line comment, then
staged thread replies, then conversation comments. GitHub Enterprise
hosts work through `gh`'s own `--hostname` support.

## Forgejo

Install [`fj`](https://codeberg.org/forgejo-contrib/forgejo-cli) and
authenticate it against your instance. Prchum speaks the
Gitea-compatible v1 REST API through a **command template** — by default:

```
fj -H {host} api {method} {path}
```

with the JSON body on stdin. If your instance standardizes on different
tooling, override `forgejo_api_command` in
[configuration](configuration.md) — the placeholders are `{host}`,
`{method}`, and `{path}` (relative to `/api/v1`) — and nothing else
changes.

Mapping notes, since Forgejo's review model differs from GitHub's:

- Reviews post with `APPROVED` / `REQUEST_CHANGES` / `COMMENT` events;
  line comments anchor by line number (`new_position` / `old_position`).
- Multi-line selections anchor on their end line.
- There is no per-comment reply endpoint: a reply becomes a positioned
  comment in a fresh `COMMENT` review at the thread's location.

## Self-hosted instances

`codeberg.org` and hosts containing `forgejo`, `gitea`, or `gitlab` are
recognized by name. An instance whose hostname says nothing
(`git.example.com`) declares its kind once in configuration:

```json
{ "forges": { "git.example.com": "forgejo" } }
```

## The review queue

File → My Review Queue (⇧⌘L) lists the open requests waiting on you —
Return or a double-click opens one. The engine follows configuration:
`gh search prs` with `is:open review-requested:@me` by default, or
Forgejo's issue search with `list_engine: "forgejo"` plus a `list_host`.
The queue's dropdown offers the default, every named filter from
`list_filters`, and a one-off custom filter typed on the spot.

## GitLab

Merge requests work through [`glab`](https://gitlab.com/gitlab-org/cli)
(`glab auth login`). GitLab has no atomic review, so submission maps:
each line comment becomes a positioned diff discussion, the summary a
note, Approve approves, and Request changes posts a "Changes requested"
note — in order, with a failure reporting how many were already
published. Suggestion fences are rewritten into GitLab's ranged form so
multi-line selections replace the whole range.

## Inside a Flatpak

Prchum borrows the CLIs you have already authenticated, which works
because it and they live in the same world. A Flatpak is a different
world: the sandbox has its own filesystem and its own `PATH`, and none
of the host's tools are on it — a plain `git` there fails with "command
not found".

So the Flatpak asks for `--talk-name=org.freedesktop.Flatpak` and runs
every subprocess through `flatpak-spawn --host`. The app notices it is
sandboxed and does this by itself; nothing needs configuring. The
alternative would be for prchum to hold credentials of its own, which is
the one thing it is built not to do.
