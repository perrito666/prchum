# Sources

Prchum passe en revue quatre sortes de source dans la même fenêtre.

## Fichiers de patch

Ouvrez un `.diff`/`.patch` (File → Open…, glissé sur l'application, ou
en ligne de commande). Les brouillons s'indexent sur le chemin absolu du
fichier : rouvrir le même fichier reprend les mêmes notes.

## Comparaisons git locales

File → Review Git Repository… choisit un dépôt et une comparaison :

- **Arbre de travail contre HEAD** — ce que montre `git diff`.
- **Index contre HEAD** — ce que montre `git diff --cached`.
- **Contre une référence de base** — `base...HEAD`, la comparaison par
  merge-base.

Chaque comparaison garde son propre brouillon : les notes sur l'index ne
se mélangent jamais aux notes contre `main`.

## Pull requests

Une URL, `owner/repo#N`, ou un simple numéro (le dépôt se déduit de
l'origin du répertoire courant). Prchum récupère le **diff canonique**
du serveur — les positions des commentaires correspondent donc toujours
à ce que la forge affiche — ainsi que les fils de revue existants
(marqueurs `◆` — ⌘R répond) et les métadonnées de la pull request
(⌘I). Voir [Forges](forges.md) pour les hôtes et l'authentification.

Si la tête bouge entre deux sessions, les brouillons sauvegardés se
réancrent d'après leur contexte capturé : une correspondance exacte
garde sa place, une correspondance textuelle unique suit le code, et
tout cas ambigu devient **orphelin** — conservé, signalé, jamais soumis.
Deviner mettrait une note sur la mauvaise ligne.

## Le format d'échange

Prchum lit et écrit le format `*.review.json` de leanreview (version 1),
détecté au contenu, jamais au nom de fichier. Un LLM écrit sa revue dans
un document autonome ; vous la triez dans Prchum — écarter, modifier,
répondre — et chaque changement réécrit le fichier sur place : en
quittant, la conversation est à jour pour le tour suivant du modèle. Les
documents inchangés font l'aller-retour à l'octet près, si bien que les
deux clients interopèrent sur les mêmes fichiers.
