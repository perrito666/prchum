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
`list_engine: "forgejo"` et un `list_host`. Le menu de la file propose
le filtre par défaut, chaque filtre nommé de `list_filters` et un
filtre ponctuel saisi sur place.

## GitLab

Les merge requests passent par
[`glab`](https://gitlab.com/gitlab-org/cli) (`glab auth login`). GitLab
n'a pas de revue atomique : la soumission se traduit — chaque
commentaire de ligne devient une discussion positionnée, le résumé une
note, Approve approuve, Request changes publie une note « Changes
requested » — dans l'ordre, un échec indiquant combien sont déjà
publiés. Les blocs de suggestion sont réécrits dans la forme à
intervalle de GitLab pour que les sélections multilignes remplacent tout
l'intervalle.

## Dans un Flatpak

Prchum emprunte les CLI que vous avez déjà authentifiées, ce qui marche
parce qu'elles et lui vivent dans le même monde. Un Flatpak est un autre
monde : le bac à sable a son propre système de fichiers et son propre
`PATH`, et aucun des outils de l'hôte ne s'y trouve — un simple `git` y
échoue avec « command not found ».

Le Flatpak demande donc `--talk-name=org.freedesktop.Flatpak` et lance
chaque sous-processus via `flatpak-spawn --host`. L'application détecte
elle-même qu'elle est confinée ; il n'y a rien à configurer.
L'alternative serait que prchum détienne ses propres identifiants, ce
qu'il est précisément conçu pour ne pas faire.
