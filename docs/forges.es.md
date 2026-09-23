# Forjas

Prchum habla con las forjas a través de sus clientes de línea de
comandos, nunca con credenciales propias: la autenticación, el
almacenamiento de tokens y los servidores de empresa siguen siendo el
problema — ya resuelto — del CLI.

## GitHub

Instale [`gh`](https://cli.github.com) y ejecute `gh auth login` una
vez. Todo viaja por `gh api`: metadatos, el diff canónico, los hilos de
revisión y el envío — una revisión atómica con todos los comentarios de
línea, luego las respuestas preparadas a hilos, luego los comentarios de
conversación. Los servidores GitHub Enterprise funcionan mediante el
`--hostname` del propio `gh`.

## Forgejo

Instale [`fj`](https://codeberg.org/forgejo-contrib/forgejo-cli) y
autentíquelo contra su instancia. Prchum habla la API REST v1
compatible con Gitea a través de una **plantilla de comando** — por
defecto:

```
fj -H {host} api {method} {path}
```

con el cuerpo JSON por stdin. Si su instancia estandariza otra
herramienta, sobrescriba `forgejo_api_command` en la
[configuración](configuration.md) — los marcadores son `{host}`,
`{method}` y `{path}` (relativo a `/api/v1`) — y nada más cambia.

Notas de correspondencia, porque el modelo de revisión de Forgejo
difiere del de GitHub:

- Las revisiones se publican con eventos `APPROVED` /
  `REQUEST_CHANGES` / `COMMENT`; los comentarios de línea se anclan por
  número de línea (`new_position` / `old_position`).
- Las selecciones multilínea se anclan en su línea final.
- No hay endpoint de respuesta por comentario: una respuesta se
  convierte en un comentario posicionado dentro de una revisión
  `COMMENT` nueva en la ubicación del hilo.

## Instancias propias

`codeberg.org` y los servidores que contienen `forgejo`, `gitea` o
`gitlab` se reconocen por el nombre. Una instancia cuyo nombre no dice
nada (`git.example.com`) declara su clase una vez en la configuración:

```json
{ "forges": { "git.example.com": "forgejo" } }
```

## La cola de revisión

File → My Review Queue (⇧⌘L) lista las solicitudes abiertas que lo
esperan — Retorno o doble clic abre una. El motor sigue la
configuración: `gh search prs` con `is:open review-requested:@me` por
defecto, o la búsqueda de issues de Forgejo con
`list_engine: "forgejo"` más un `list_host`. El desplegable de la cola
ofrece el filtro por defecto, cada filtro con nombre de `list_filters`
y un filtro puntual escrito al momento.

## GitLab

Los merge requests funcionan a través de
[`glab`](https://gitlab.com/gitlab-org/cli) (`glab auth login`). GitLab
no tiene revisión atómica, así que el envío se traduce: cada comentario
de línea se convierte en una discusión posicionada, el resumen en una
nota, Approve aprueba y Request changes publica una nota de «Changes
requested» — en orden, y un fallo informa cuántos se publicaron ya. Los
bloques de sugerencia se reescriben a la forma con rango de GitLab para
que las selecciones multilínea reemplacen el rango completo.

## Revisar un solo commit

Cuando revisas un solo commit de un pedido (ver
[La revisión](reviewing.md)), sus comentarios de línea están
posicionados en el diff de ese commit, así que se envían fijados a él —
como los publica la vista de un solo commit de cada forja:

| Forja | Commits desde | El diff del commit | Fijado por |
| --- | --- | --- | --- |
| GitHub | `pulls/N/commits` | `commits/SHA` como diff | el `commit_id` de la revisión |
| GitLab | `merge_requests/N/commits` | `repository/commits/SHA/diff` | la posición de cada discusión: `base_sha` y `start_sha` el padre, `head_sha` el commit |
| Forgejo | `pulls/N/commits` | `git/commits/SHA.diff` | el `commit_id` de la revisión |

El diff de un commit se toma respecto de su primer padre. Nada se
adivina: si la forja rechaza la posición de un comentario, el envío
informa el error y cada borrador que no aceptó se queda para
reintentar. GitHub lista como mucho 250 commits de un pull request, y
el selector lo avisa cuando hay más.
