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

Une sélection doit tenir sur un seul côté, à la façon de GitHub : un
bloc de changements s'ancre à DROITE (les suppressions ne font
simplement pas partie de ce côté), une sélection de suppressions seules
s'ancre à GAUCHE. Les brouillons se signalent par `●` dans la marge,
avec la note en ligne ; les fils existants du serveur, par `◆`.

Écarter n'est pas supprimer : le verdict voyage avec la revue — c'est
l'information dont l'autre côté d'une conversation a le plus besoin —
mais un commentaire écarté n'est jamais soumis.

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
