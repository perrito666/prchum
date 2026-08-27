# Configuration

One JSON file:
`~/Library/Application Support/Prchum/config.json` — hand-editable, with
textchum's escape-hatch rules: a missing file means defaults; a broken
file means defaults plus a logged warning, and the file on disk is never
touched; unknown keys are tolerated.

```json
{
  "keys": {
    "next-hunk": "cmd+alt+n",
    "toggle-wrap": ""
  },
  "keymap": "mine",
  "keymaps": {
    "mine": { "reply": "cmd+alt+r" }
  },
  "forges": { "git.example.com": "forgejo" },
  "forgejo_api_command": "fj -H {host} api {method} {path}",
  "list_engine": "gh",
  "list_filter": "is:open review-requested:@me",
  "list_host": ""
}
```

## Keys

`keymap` selects a named keymap from `keymaps` as the base layer;
top-level `keys` override it — overrides of overrides. An unknown name
is logged and contributes nothing.

`keys` maps action names to key specs. A spec is modifiers plus a key:
`cmd`/`command`, `alt`/`opt`/`option`, `ctrl`/`control`, `shift`, then a
single character or a named key (`up`, `down`, `left`, `right`,
`pageup`, `pagedown`, `home`, `end`, `return`, `space`, `tab`, `esc`,
`delete`). An empty string unbinds the default. Unknown actions and
unparseable specs are logged and the default stays.

Action names: `open`, `open-pr`, `open-git`, `review-queue`, `export`,
`next-change`, `prev-change`, `next-hunk`, `prev-hunk`, `next-file`,
`prev-file`, `toggle-sidebar`, `toggle-layout`, `toggle-wrap`,
`toggle-syntax`, `toggle-fold`, `expand-all`, `collapse-all`, `find`,
`comment`, `edit-comment`, `delete-comment`, `dismiss-comment`,
`reply`, `pr-info`, `submit`, `open-at-caret`, `suggest`, `comments`,
`general`, `toggle-context` — every menu item names its action.

## Appearance and theme

- `appearance` — `system` (default), `light`, or `dark`.
- `theme` — a built-in (`default`, `high-contrast`, `graphite`,
  `molokai`, `solarized`, `dracula`, `gruvbox` — each with a light and a
  dark palette) or the name of a `themes/<name>.json` file next to
  config.json. Settings (⌘,) writes both through.

## Forges

- `forges` — host → kind (`github` | `gitlab` | `forgejo`) for
  self-hosted instances whose hostname says nothing.
- `forgejo_api_command` — the Forgejo transport template; empty means
  the built-in `fj` default. See [Forges](forges.md).

## Local editing

- `clones` — `owner/repo` → the local clone that holds it
  (`{"perrito666/prchum": "/Users/me/src/prchum"}`), matched
  case-insensitively. Settings (⌘,) manages the list, and Edit File
  Locally offers to pick one when a repository has none.
- `editor_command` — how to open a file: a URL or a command, with
  `{path}`, `{line}`, and `{dir}` placeholders. Empty means
  `textchum://open?path={path}&line={line}`; `code -g {path}:{line}` and
  `nvim +{line} {path}` are the command form.

Worktrees prchum creates live in `worktrees/` beside the drafts, and are
tracked in `worktrees.json` — the record of what prchum owns, and so
what it may remove when a request is finished.

## Discovery

- `list_engine` — `gh` (default) or `forgejo`.
- `list_filter` — a GitHub search query, or Forgejo query-string
  qualifiers; empty means the engine's default
  (`is:open review-requested:@me`).
- `list_filters` — named filters (`{"bugs": "is:open label:bug"}`),
  pickable in the review queue's dropdown; Settings (⌘,) manages them,
  and the queue also takes a one-off custom filter.
- `list_host` — the Forgejo host the queue searches (required for the
  `forgejo` engine).

## State on disk

- Drafts: `~/Library/Application Support/Prchum/drafts/`, one JSON file
  per source, written atomically on every change.
- Exchange sessions also rewrite their own `*.review.json` in place.
