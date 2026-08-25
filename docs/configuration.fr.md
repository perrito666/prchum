# Configuration

Un seul fichier JSON :
`~/Library/Application Support/Prchum/config.json` — modifiable à la
main, avec les règles d'échappatoire de textchum : fichier absent =
valeurs par défaut ; fichier cassé = valeurs par défaut plus un
avertissement journalisé, et le fichier sur disque n'est jamais touché ;
les clés inconnues sont tolérées.

```json
{
  "keys": {
    "next-hunk": "cmd+alt+n",
    "toggle-wrap": ""
  },
  "keymap": "mien",
  "keymaps": {
    "mien": { "reply": "cmd+alt+r" }
  },
  "forges": { "git.example.com": "forgejo" },
  "forgejo_api_command": "fj -H {host} api {method} {path}",
  "list_engine": "gh",
  "list_filter": "is:open review-requested:@me",
  "list_host": ""
}
```

## Touches

`keymap` choisit une table nommée de `keymaps` comme couche de base ;
les `keys` de premier niveau la surchargent — des couches de couches. Un
nom inconnu est journalisé et n'apporte rien.

`keys` associe des noms d'action à des raccourcis. Un raccourci, ce sont
des modificateurs plus une touche : `cmd`/`command`,
`alt`/`opt`/`option`, `ctrl`/`control`, `shift`, puis un caractère seul
ou une touche nommée (`up`, `down`, `left`, `right`, `pageup`,
`pagedown`, `home`, `end`, `return`, `space`, `tab`, `esc`, `delete`).
Une chaîne vide retire l'assignation par défaut. Les actions inconnues
et les raccourcis illisibles sont journalisés et la valeur par défaut
reste.

Noms d'action : `open`, `open-pr`, `open-git`, `review-queue`,
`export`, `next-change`, `prev-change`, `next-hunk`, `prev-hunk`,
`next-file`, `prev-file`, `toggle-sidebar`, `toggle-layout`,
`toggle-wrap`, `toggle-syntax`, `toggle-fold`, `expand-all`,
`collapse-all`, `find`, `comment`, `edit-comment`, `delete-comment`,
`dismiss-comment`, `reply`, `open-at-caret`, `suggest`, `comments`,
`general`, `toggle-context`, `toggle-fold`, `expand-all`,
`collapse-all`… — every menu item names its action; the full list
lives in the Keymap source.

## Apparence et thème

- `appearance` — `system` (par défaut), `light` ou `dark`.
- `theme` — un intégré (`default`, `high-contrast`, `graphite`,
  `molokai`, `solarized`, `dracula`, `gruvbox` — chacun avec une palette
  claire et une sombre) ou le nom d'un fichier `themes/<name>.json` à
  côté de config.json. Settings (⌘,) écrit les deux.

## Forges

- `forges` — hôte → nature (`github` | `gitlab` | `forgejo`) pour les
  instances auto-hébergées au nom muet.
- `forgejo_api_command` — le gabarit de transport Forgejo ; vide =
  le `fj` par défaut. Voir [Forges](forges.md).

## Découverte

- `list_engine` — `gh` (par défaut) ou `forgejo`.
- `list_filter` — une requête de recherche GitHub, ou des qualificatifs
  de query string Forgejo ; vide = le défaut du moteur
  (`is:open review-requested:@me`).
- `list_host` — l'hôte Forgejo que la file interroge (requis avec le
  moteur `forgejo`).

## État sur disque

- Brouillons : `~/Library/Application Support/Prchum/drafts/`, un
  fichier JSON par source, écrit atomiquement à chaque changement.
- Les sessions d'échange réécrivent en outre leur propre
  `*.review.json` sur place.
