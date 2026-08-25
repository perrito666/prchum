# Configuración

Un solo archivo JSON:
`~/Library/Application Support/Prchum/config.json` — editable a mano,
con las reglas de escape de textchum: un archivo ausente significa
valores por defecto; un archivo roto significa valores por defecto más
un aviso registrado, y el archivo en disco nunca se toca; las claves
desconocidas se toleran.

```json
{
  "keys": {
    "next-hunk": "cmd+alt+n",
    "toggle-wrap": ""
  },
  "keymap": "mio",
  "keymaps": {
    "mio": { "reply": "cmd+alt+r" }
  },
  "forges": { "git.example.com": "forgejo" },
  "forgejo_api_command": "fj -H {host} api {method} {path}",
  "list_engine": "gh",
  "list_filter": "is:open review-requested:@me",
  "list_host": ""
}
```

## Teclas

`keymap` selecciona un mapa con nombre de `keymaps` como capa base;
las `keys` de nivel superior lo sobrescriben — capas de capas. Un nombre
desconocido se registra y no aporta nada.

`keys` asocia nombres de acción con especificaciones de tecla. Una
especificación son modificadores más una tecla: `cmd`/`command`,
`alt`/`opt`/`option`, `ctrl`/`control`, `shift`, y después un carácter
único o una tecla con nombre (`up`, `down`, `left`, `right`, `pageup`,
`pagedown`, `home`, `end`, `return`, `space`, `tab`, `esc`, `delete`).
Una cadena vacía desasigna el valor por defecto. Las acciones
desconocidas y las especificaciones ilegibles se registran y el valor
por defecto se mantiene.

Nombres de acción: `open`, `open-pr`, `open-git`, `review-queue`,
`export`, `next-change`, `prev-change`, `next-hunk`, `prev-hunk`,
`next-file`, `prev-file`, `toggle-sidebar`, `toggle-layout`,
`toggle-wrap`, `toggle-syntax`, `toggle-fold`, `expand-all`,
`collapse-all`, `find`, `comment`, `edit-comment`, `delete-comment`,
`dismiss-comment`, `reply`, `open-at-caret`, `suggest`, `comments`,
`general`, `toggle-context`, `toggle-fold`, `expand-all`,
`collapse-all`… — every menu item names its action; the full list
lives in the Keymap source.

## Apariencia y tema

- `appearance` — `system` (por defecto), `light` o `dark`.
- `theme` — uno integrado (`default`, `high-contrast`, `graphite`,
  `molokai`, `solarized`, `dracula`, `gruvbox` — cada uno con paleta
  clara y oscura) o el nombre de un archivo `themes/<name>.json` junto a
  config.json. Settings (⌘,) escribe ambos.

## Forjas

- `forges` — servidor → clase (`github` | `gitlab` | `forgejo`) para
  instancias propias cuyo nombre no dice nada.
- `forgejo_api_command` — la plantilla de transporte de Forgejo; vacía
  significa el `fj` por defecto. Véase [Forjas](forges.md).

## Descubrimiento

- `list_engine` — `gh` (por defecto) o `forgejo`.
- `list_filter` — una consulta de búsqueda de GitHub, o calificadores
  de query string de Forgejo; vacío significa el valor por defecto del
  motor (`is:open review-requested:@me`).
- `list_host` — el servidor Forgejo donde busca la cola (obligatorio
  con el motor `forgejo`).

## Estado en disco

- Borradores: `~/Library/Application Support/Prchum/drafts/`, un
  archivo JSON por fuente, escrito atómicamente en cada cambio.
- Las sesiones de intercambio reescriben además su propio
  `*.review.json` en su sitio.
