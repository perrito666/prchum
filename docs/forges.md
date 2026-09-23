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

## Reviewing one commit

When you review a single commit of a request (see
[Reviewing](reviewing.md)), its line comments are positioned in that
commit's diff, so they are submitted pinned to it — the way each
forge's own single-commit view posts them:

| Forge | Commits from | The commit's diff | Pinned by |
| --- | --- | --- | --- |
| GitHub | `pulls/N/commits` | `commits/SHA` as a diff | the review's `commit_id` |
| GitLab | `merge_requests/N/commits` | `repository/commits/SHA/diff` | each discussion's position: `base_sha` and `start_sha` the parent, `head_sha` the commit |
| Forgejo | `pulls/N/commits` | `git/commits/SHA.diff` | the review's `commit_id` |

A commit's diff is taken against its first parent. Nothing is guessed:
if the forge refuses a comment's position, the submission reports the
error and every draft it did not accept stays for a retry. GitHub lists
at most 250 commits of a pull request, and says so in the picker when
there are more.
