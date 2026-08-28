# La revue

Chaque opération est une action nommée : un élément de menu avec son
équivalent clavier natif de macOS, réassignable via la table `keys` de
la [configuration](configuration.md). La souris fonctionne partout, mais
rien ne l'exige. La barre d'outils de la fenêtre porte les actions
courantes pour les jours à la souris, et ⇧⌘H revient à l'écran
d'accueil ; les feuilles se confirment avec ⌘↩ (Retour tape un saut de
ligne dans le corps).

## Navigation et affichage

| Par défaut | Action |
| --- | --- |
| ⌘↓ / ⌘↑ | changement suivant / précédent |
| ⌥⌘↓ / ⌥⌘↑ | hunk suivant / précédent |
| ⇧⌘↓ / ⇧⌘↑ | fichier suivant / précédent |
| ⌘F | rechercher dans le diff (la barre de recherche native) |
| ⌥⌘T | vue unifiée ↔ scindée |
| ⌥⌘C | contexte complet : le fichier entier avec les hunks superposés |
| ⌥⌘← | plier / déplier le hunk courant |
| ⇧⌥⌘← / ⇧⌥⌘→ | plier / déplier tous les hunks |
| ⌥⌘S | cycler la coloration : syntaxe + teintes → teintes seules → brut |
| ⌥⌘W | replier les lignes longues |
| ⌃⌘S | afficher ou masquer la barre des fichiers |

La coloration syntaxique lance une passe tree-sitter par côté et par
hunk : les constructions multilignes se colorent correctement des deux
côtés d'un changement. Quatorze langages sont intégrés.

En vue scindée, les deux côtés occupent des panneaux parallèles ; le
panneau où se trouve le curseur décide du côté que vise un commentaire.

## Commentaires

| Par défaut | Action |
| --- | --- |
| ⌘↩ | commenter la ligne du curseur ou la sélection |
| ⌘E | modifier le brouillon sous le curseur |
| ⌘⌫ | supprimer le brouillon sous le curseur |
| ⇧⌘X | écarter ↔ restaurer (conservé ; jamais soumis tant qu'écarté) |
| ⌥⌘↩ | suggérer un changement : le code sélectionné prérempli dans un bloc ```suggestion |
| ⌘R | répondre — au fil du serveur ou à la conversation du brouillon |
| ⌘L | le navigateur de revue : chaque brouillon et fil ; Retour saute |
| ⌃⌘E | modifier le fichier courant localement (voir plus bas) |

Une sélection doit tenir sur un seul côté, à la façon de GitHub : un
bloc de changements s'ancre à DROITE (les suppressions ne font
simplement pas partie de ce côté), une sélection de suppressions seules
s'ancre à GAUCHE. Les brouillons se signalent par `●` dans la marge,
avec la note en ligne ; les fils existants du serveur, par `◆`.

Écarter n'est pas supprimer : le verdict voyage avec la revue — c'est
l'information dont l'autre côté d'une conversation a le plus besoin —
mais un commentaire écarté n'est jamais soumis.

## L'édition locale

⌃⌘E ouvre le fichier sous le curseur dans votre éditeur, dans une copie
locale de la branche en revue — à la ligne du curseur quand cette ligne
existe dans le fichier (une suppression l'ouvre sans ligne).

La copie vient du clone que vous désignez dans la
[configuration](configuration.md) : si la branche y est déjà extraite —
dans le clone lui-même ou dans un worktree à vous — c'est celui-là qui
sert, intact ; sinon prchum crée son propre worktree à côté de son état,
en récupérant la tête de la demande quand la branche n'est pas encore
locale. Seuls les worktrees créés par prchum sont supprimés, et
seulement quand la demande est fusionnée, fermée ou disparue.

Une comparaison git n'a besoin d'aucun clone : c'est déjà une copie de
travail, le fichier s'ouvre sur place.

## La soumission

⇧⌘↩ ouvre la feuille de soumission sur une session de pull request ;
⌥⌘A l'ouvre avec **Approve** présélectionné et ⌥⌘R avec **Request
changes** — la feuille confirme dans tous les cas. Elle montre
les décomptes, le sélecteur d'événement (Comment / Approve / Request
changes), le résumé et un avertissement explicite pour les commentaires
orphelins, qui ne sont jamais soumis. Rien ne part avant cette
confirmation.

La soumission résiste aux nouvelles tentatives : l'application
enregistre exactement ce que le serveur a accepté, même si une étape
ultérieure échoue ; une nouvelle tentative n'envoie que ce qui reste en
attente — jamais un doublon.

## L'export

⇧⌘E écrit vos notes dans un fichier : du Markdown groupé par fichier,
ou — avec un nom en `.json` — un document d'échange autonome (voir
[Sources](sources.md)) qui embarque le patch.

## Depuis le terminal

Prchum est fait pour être appelé comme on appelle `git diff`, et prend
donc les mêmes formes d'argument :

```sh
prchum                  # ce qu'afficherait git diff
prchum --staged         # ce qu'afficherait git diff --staged
prchum main             # cette branche face à main
prchum v1..v2           # un intervalle
prchum change.diff      # un correctif ou un document d'échange
prchum 418              # la demande 418 de l'origin de ce dépôt
prchum owner/repo#418   # un dépôt explicite
```

`git prchum` fait la même chose, parce que git traite tout `git-*` du
PATH comme une sous-commande — et il s'exécute depuis la racine du
dépôt, donc il veut dire la même chose dans un sous-répertoire qu'à la
racine.

Pour en faire la commande réflexe, donnez un alias à git :

```sh
git config --global alias.d '!git prchum'
```

`git d` ouvre alors ce que `git diff` aurait imprimé, et `git d main`
compare à une branche. Une réserve : `git prchum --help` déclenche
l'aide de git, qui cherche une page de manuel. Utilisez `prchum --help`.

Sous macOS la commande s'installe depuis **Prchum → Install
Command-Line Tool…**, ou avec `make install-cli` depuis un checkout.
Sous Linux, les paquets l'installent.

!!! note "Elle s'appelait `pr`"

    Ce qui était une erreur : `pr` est le paginateur POSIX, il a une page
    de manuel, et `/usr/local/bin` passe avant `/usr/bin` dans le PATH par
    défaut — l'installer là masquait donc silencieusement un outil
    standard. Si vous avez l'ancien, `rm /usr/local/bin/pr` remet les
    choses en place.
