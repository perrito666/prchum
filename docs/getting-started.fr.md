# Premiers pas

## Installer

Téléchargez `Prchum.app` depuis les
[releases](https://github.com/perrito666/prchum/releases) (non signée :
clic droit → Ouvrir au premier lancement ; Apple Silicon, macOS 14+),
ou compilez depuis les sources :

```sh
make run ARGS=changement.diff   # compile cœur + app et ouvre un patch
make app                        # un Prchum.app double-cliquable dans dist/
make check                      # ce que la CI exécute : tests, smoke test, dérive de l'en-tête
```

La commande de terminal `pr` s'installe depuis le menu de l'application
(Prchum → Install pr Command…) ou avec `make install-cli`.

La compilation demande une chaîne d'outils Rust et Xcode. La revue de
pull requests demande le CLI de la forge installé et authentifié :
[`gh`](https://cli.github.com) (`gh auth login`) pour GitHub,
[`fj`](https://codeberg.org/forgejo-contrib/forgejo-cli) pour Forgejo.
Les patchs et les comparaisons git locales n'exigent rien de plus que
`git`.

## Première revue

Lancée sans cible, l'application demande quoi revoir : une pull
request, votre file de revue, un fichier de patch ou un dépôt git.
Chaque porte figure aussi dans le menu File, et tout fonctionne en ligne
de commande :

```sh
Prchum changement.diff                       # un fichier de patch
Prchum 418                                   # la PR 418 de l'origin du dépôt courant
Prchum owner/repo#418                        # dépôt explicite
Prchum https://github.com/owner/repo/pull/418
```

Une fois le diff ouvert :

1. **Naviguez.** ⌘↓/⌘↑ sautent de changement en changement, ⌥⌘↓/⌥⌘↑ de
   hunk en hunk, ⇧⌘↓/⇧⌘↑ de fichier en fichier — ou cliquez sur un
   fichier dans la barre latérale.
2. **Commentez.** Placez le curseur (ou sélectionnez des lignes) et
   pressez ⌘↩. Écrivez la note, pressez Comment. La ligne reçoit un
   marqueur `●` et la note s'affiche en ligne dessous.
3. **Soumettez ou exportez.** Sur une pull request, ⇧⌘↩ ouvre la
   feuille de soumission — rien ne part avant confirmation. Partout,
   ⇧⌘E exporte vos notes en Markdown (ou en document d'échange si le
   nom finit en `.json`).

Les brouillons se sauvegardent seuls par source et se rechargent à la
prochaine ouverture de la même comparaison.
