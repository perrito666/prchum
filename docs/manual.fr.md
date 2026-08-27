# Visite guidée

Une revue du début à la fin, avec les fenêtres que vous verrez vraiment.
Les captures suivent cette page : claires en mode clair, sombres en mode
sombre. Elles utilisent le thème de coloration `default`, le dépôt en
revue est un petit générateur de coups d'échecs, et la relectrice est
`ada`.

## L'écran d'accueil

Prchum s'ouvre sur l'écran d'accueil : quatre façons d'entrer en haut
et, dessous, les revues déjà ouvertes.

![L'écran d'accueil](images/home-light.png#only-light)
![L'écran d'accueil](images/home-dark.png#only-dark)

L'historique retient d'où venait chaque revue et si vous l'avez envoyée.
Les lignes des demandes fusionnées ou fermées depuis sont élaguées, et
avec elles les worktrees que prchum avait créés.

## La file de revue

⇧⌘L demande à votre forge quelles demandes vous attendent.

![La file de revue](images/review-queue-light.png#only-light)
![La file de revue](images/review-queue-dark.png#only-dark)

Le sélecteur en haut choisit le filtre. Les filtres nommés viennent de
la table `list_filters` de votre configuration, le filtre par défaut
s'exécute quand vous n'en choisissez aucun, et **Custom…** accepte un
filtre tapé sur le moment pour le reste de la session.

![Le sélecteur de filtres](images/queue-filters-light.png#only-light)
![Le sélecteur de filtres](images/queue-filters-dark.png#only-dark)

Return, ou un double clic, ouvre la demande sélectionnée.

## Lire un diff

La fenêtre de revue, c'est la barre latérale des fichiers modifiés, la
barre d'outils et le diff.

![Une fenêtre de revue](images/review-window-light.png#only-light)
![Une fenêtre de revue](images/review-window-dark.png#only-dark)

La barre latérale compte les ajouts et les suppressions par fichier, et
marque les fichiers porteurs de commentaires. ⌘↓ et ⌘↑ parcourent les
changements, ⌥⌘↓ et ⌥⌘↑ les hunks, ⇧⌘↓ et ⇧⌘↑ les fichiers. La
coloration syntaxique fait une passe tree-sitter par côté de chaque
hunk : une construction sur plusieurs lignes se colore correctement sur
l'ancien texte comme sur le nouveau.

Ouvrir une pull request donne la même chose — la barre de titre nomme la
demande au lieu de la comparaison.

![Une pull request](images/pull-request-light.png#only-light)
![Une pull request](images/pull-request-dark.png#only-dark)

⌘I affiche la description de la demande, rendue en Markdown, avec la
branche cible et un bouton pour l'ouvrir dans un navigateur.

![Informations sur la pull request](images/pr-info-light.png#only-light)
![Informations sur la pull request](images/pr-info-dark.png#only-dark)

## Vue côte à côte

⌥⌘T place les deux côtés dans des panneaux parallèles. Le panneau où se
trouve le curseur décide du côté visé par un nouveau commentaire.

![Vue côte à côte](images/split-view-light.png#only-light)
![Vue côte à côte](images/split-view-dark.png#only-dark)

## Contexte du fichier entier

Un diff montre trois lignes autour de chaque changement, souvent trois
lignes de trop peu.

![Les hunks seuls](images/hunk-view-light.png#only-light)
![Les hunks seuls](images/hunk-view-dark.png#only-dark)

⌥⌘C récupère le fichier entier et y replace les hunks, pour que vous
lisiez le changement là où il vit. Le code hors du diff est coloré lui
aussi, et la récupération se fait hors du fil principal : la fenêtre
reste vivante pendant ce temps.

![Contexte du fichier entier](images/context-view-light.png#only-light)
![Contexte du fichier entier](images/context-view-dark.png#only-dark)

## Commenter

⌘↩ commente la ligne sous le curseur, ou la sélection.

![Le compositeur de commentaires](images/comment-composer-light.png#only-light)
![Le compositeur de commentaires](images/comment-composer-dark.png#only-dark)

Une sélection sur plusieurs lignes devient un commentaire de plage,
telle que la forge la comprend.

![Un commentaire multi-lignes](images/multiline-comment-light.png#only-light)
![Un commentaire multi-lignes](images/multiline-comment-dark.png#only-dark)

Les commentaires ne sont pas attachés à une ligne à l'écran. Ils
s'ancrent à un emplacement sémantique — fichier, côté, plage de lignes,
et une courte ancre de contexte avec une empreinte du contenu de la
ligne — et c'est pourquoi un brouillon survit au déplacement de la
branche sous lui.

Les brouillons et les fils déjà présents sur la demande apparaissent en
ligne, encadrés, leur Markdown rendu.

![Un fil de commentaires](images/comment-light.png#only-light)
![Un fil de commentaires](images/comment-dark.png#only-dark)

⌘R répond dans un fil, ⌘E modifie le brouillon sous le curseur, ⇧⌘X en
écarte un (gardé en local, jamais envoyé) et ⌘⌫ le supprime.

## Le navigateur de revue

⌘L liste tous les brouillons et fils de la revue ; Return saute à celui
que vous choisissez.

![Le navigateur de revue](images/navigator-light.png#only-light)
![Le navigateur de revue](images/navigator-dark.png#only-dark)

## Envoyer

⇧⌘↩ ouvre la feuille d'envoi : combien de commentaires et de réponses
vont partir, une zone pour le résumé de la revue, et l'événement —
commenter, approuver ou demander des changements. ⌥⌘A et ⌥⌘R ouvrent la
même feuille avec approuver ou demander des changements déjà choisi.

Les commentaires que la forge accepte quittent vos brouillons locaux au
fur et à mesure : un envoi qui échoue à mi-chemin se reprend sans rien
publier deux fois. Les brouillons orphelins — ceux dont le code a
disparu — ne sont jamais envoyés, et la feuille le dit avant que vous
vous engagiez.

Quand tout part correctement, vous recevez une notification plutôt
qu'une boîte de dialogue : il n'y a rien à décider, donc rien ne
s'interpose. Un envoi qui n'est passé qu'à moitié pose toujours la
question, parce que c'est quelque chose qu'il faut voir.

## Réglages

⌘, garde ce qui vaut la peine d'être changé.

![Réglages](images/settings-light.png#only-light)
![Réglages](images/settings-dark.png#only-dark)

Apparence et thème ; le nom auquel vos brouillons sont attribués ; les
filtres de découverte, celui par défaut et les filtres nommés ; le
modèle d'éditeur ; et la table qui relie les dépôts de la forge à des
clones locaux, dont **Edit File Locally** (⌃⌘E) se sert pour sortir la
branche et ouvrir le fichier là où est votre curseur.

Tout cela est écrit dans `config.json`, qui reste modifiable à la main :
les clés inconnues survivent à chaque enregistrement, et un fichier que
prchum ne sait pas lire n'est jamais écrasé.

## Sous Linux

La même revue, dans l'interface GTK. Le cœur de prchum est une seule
bibliothèque portable ; ce qui change d'une plateforme à l'autre, c'est
la présentation, et cela change délibérément : ceci est une application
GNOME, pas une application Mac déguisée.

![La fenêtre de revue sous Linux](images/linux-review-light.png#only-light)
![La fenêtre de revue sous Linux](images/linux-review-dark.png#only-dark)

Les lignes sont identiques parce que c'est le cœur qui les décide : les
mêmes fichiers, les mêmes marqueurs et numéros de ligne, les mêmes
couleurs tree-sitter issues de la même table de styles. Ce qui diffère,
c'est tout ce qui les entoure : une barre d'en-tête libadwaita portant
le titre et le fichier, les boutons de fenêtre propres à GNOME, et ses
couleurs d'accent dans les compteurs de la barre latérale. Elle suit le
réglage clair ou sombre du bureau, comme ci-dessus.

Commenter fonctionne comme sous macOS, parce que c'est le même cœur qui
s'en charge : **Ctrl+Return** ouvre le compositeur sur la ligne sous le
curseur, **Ctrl+E** modifie un brouillon, **Ctrl+Suppr** le supprime et
**Ctrl+Maj+X** l'écarte. Les brouillons apparaissent en ligne sous la
ligne à laquelle ils se rattachent et survivent à la fermeture de la
fenêtre : ils sont écrits à côté de la configuration, dans
`~/.local/share/prchum`, suivant la disposition XDG et non celle de
macOS.

Les raccourcis diffèrent, à dessein. Les actions portent les mêmes noms
sur les deux plateformes et les mêmes entrées dans la table `keys`, mais
sous GNOME on appuie sur Ctrl là où sur Mac on appuie sur Command : les
valeurs par défaut sont donc en Ctrl. **Ctrl+↑/↓** parcourt les
changements, **Ctrl+Maj+↑/↓** les fichiers.

Le reste y est aussi. **Ctrl+Alt+C** replace les hunks dans le fichier
entier ; **Ctrl+Maj+T** met les deux côtés dans des panneaux parallèles
qui défilent ensemble ; **Ctrl+Maj+L** demande à la forge ce qui attend
et ouvre ce que vous choisissez ; **Ctrl+,** garde les réglages, dans le
même `config.json` qu'écrit l'application macOS ; et
**Ctrl+Maj+Return** envoie, avec la même sûreté en cas de reprise : ce
que la forge a accepté quitte vos brouillons même si une étape
ultérieure échoue.

**Ctrl+R** répond à un fil que la demande porte déjà, ou à l'un de vos
brouillons, et **Ctrl+Maj+P** ouvre la conversation : les commentaires
qui appartiennent à la demande plutôt qu'à une ligne.

## Partager un lien

Deux choses valent la peine d'être envoyées : où vous regardez dans le
code, et de quelle conversation vous parlez.

**Ctrl+Maj+C** (⇧⌘C sous macOS) copie un lien permanent vers la ligne
sous le curseur. Il pointe vers le fichier tel qu'il est au commit de
tête de la demande, pas vers l'onglet des fichiers : les ancres que
chaque forge pose sur un diff s'écrivent différemment et bougent quand
la demande est mise à jour, alors qu'un blob à un commit précis
fonctionne pour qui vous l'envoyez, et continue de fonctionner.

**Ctrl+Maj+K** (⇧⌘K) copie le lien propre à la forge vers le fil sous le
curseur, quand vous parlez de *cette conversation* et non de cette ligne.

Avec **Alt** (Option sous macOS), chacun ouvre le lien au lieu de le
copier : Ctrl+Alt+Maj+C et Ctrl+Alt+Maj+K. Alt veut dire « ouvrir plutôt
que copier » partout, d'où le fait que la paire du fil soit sur K et ne
partage pas le C.

Les quatre figurent aussi dans le **menu contextuel** du diff, pour ne
rien avoir à retenir. Sous macOS le menu affiche l'un de chaque paire et
échange copier contre ouvrir tant qu'Option est enfoncée, comme le font
les éléments alternatifs ; sous Linux les quatre sont listés. Dans les
deux cas le curseur se place d'abord là où vous avez cliqué, si bien que
l'action porte sur la ligne visée.

Les deux exigent une pull request derrière la revue : un correctif ou
une comparaison locale n'a nulle part où pointer, et le dit plutôt que
de copier quelque chose d'inutile.
