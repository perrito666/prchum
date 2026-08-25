# Prchum

Un client de revue de code natif pour macOS. Passez en revue un
**fichier de patch**, une **comparaison git locale**, une **pull request
GitHub** ou une **pull request Forgejo** dans la même application :
parcourez le diff, attachez des commentaires brouillons ancrés à des
emplacements sémantiques, puis exportez vos notes en Markdown ou en
document d'échange — ou soumettez-les comme une vraie revue.

Prchum est la fonctionnalité de
[leanreview](https://github.com/perrito666/leanreview) reconstruite sur
l'architecture de [textchum](https://github.com/perrito666/textchum) :
un cœur compilé portable (Rust) derrière une coquille entièrement
native (Swift + AppKit), qui se rejoignent sur une interface C. Le cœur
possède le diff et l'état de la revue ; la coquille, la plateforme.

C'est un client de revue, pas un client git : les `git`, `gh` et `fj`
installés s'occupent de la sémantique du dépôt et de la forge — Prchum
ne stocke jamais de jeton. Son domaine : la navigation, le rendu, l'état
de la revue et les commentaires.

## La règle que tout respecte

Les lignes rendues ne sont jamais des emplacements canoniques de
commentaires. Chaque commentaire s'ancre à un **emplacement**
sémantique — chemin, côté (LEFT/RIGHT), plage de lignes et texte
environnant — et survit donc aux changements de disposition, au pliage,
au basculement unifié↔scindé et, grâce à une relocalisation prudente, au
déplacement de la tête de la pull request entre deux sessions.

## Pour continuer

- [Premiers pas](getting-started.md) — installer, compiler, première
  revue.
- [La revue](reviewing.md) — navigation, commentaires, soumission.
- [Sources](sources.md) — patchs, git, pull requests, le format
  d'échange.
- [Forges](forges.md) — GitHub, Forgejo et instances auto-hébergées.
- [Configuration](configuration.md) — touches, forges, découverte.
