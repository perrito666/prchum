# Forges

Prchum parle aux forges via leurs clients en ligne de commande, jamais
avec ses propres identifiants : l'authentification, le stockage des
jetons et les hôtes d'entreprise restent le problème — déjà résolu — du
CLI.

## GitHub

Installez [`gh`](https://cli.github.com) et lancez `gh auth login` une
fois. Tout passe par `gh api` : métadonnées, diff canonique, fils de
revue et soumission — une revue atomique portant tous les commentaires
de ligne, puis les réponses préparées aux fils, puis les commentaires de
conversation. Les hôtes GitHub Enterprise fonctionnent grâce au
`--hostname` de `gh` lui-même.

## Forgejo

Installez [`fj`](https://codeberg.org/forgejo-contrib/forgejo-cli) et
authentifiez-le contre votre instance. Prchum parle l'API REST v1
compatible Gitea à travers un **gabarit de commande** — par défaut :

```
fj -H {host} api {method} {path}
```

avec le corps JSON sur stdin. Si votre instance standardise un autre
outil, remplacez `forgejo_api_command` dans la
[configuration](configuration.md) — les variables sont `{host}`,
`{method}` et `{path}` (relatif à `/api/v1`) — et rien d'autre ne
change.

Notes de correspondance, le modèle de revue de Forgejo différant de
celui de GitHub :

- Les revues partent avec les événements `APPROVED` /
  `REQUEST_CHANGES` / `COMMENT` ; les commentaires de ligne s'ancrent
  par numéro de ligne (`new_position` / `old_position`).
- Les sélections multilignes s'ancrent sur leur dernière ligne.
- Pas d'endpoint de réponse par commentaire : une réponse devient un
  commentaire positionné dans une nouvelle revue `COMMENT` à
  l'emplacement du fil.

## Instances auto-hébergées

`codeberg.org` et les hôtes contenant `forgejo`, `gitea` ou `gitlab`
sont reconnus à leur nom. Une instance au nom muet
(`git.example.com`) déclare sa nature une fois dans la configuration :

```json
{ "forges": { "git.example.com": "forgejo" } }
```

## La file de revue

File → My Review Queue (⇧⌘L) liste les demandes ouvertes qui vous
attendent — Retour ou un double-clic en ouvre une. Le moteur suit la
configuration : `gh search prs` avec `is:open review-requested:@me` par
défaut, ou la recherche d'issues de Forgejo avec
`list_engine: "forgejo"` et un `list_host`.

## GitLab

Pas encore pris en charge : la couture est en place et les références de
merge request s'analysent, mais en ouvrir une l'annonce clairement au
lieu d'échouer obscurément.
