# Recorrido guiado

Un paseo por una revisión de principio a fin, con las ventanas que vas a
ver de verdad. Las capturas siguen a esta página: claras en modo claro,
oscuras en modo oscuro. Usan el tema de sintaxis `default`, el
repositorio en revisión es un pequeño generador de jugadas de ajedrez, y
quien revisa es `ada`.

## La pantalla de inicio

Prchum abre en la pantalla de inicio: cuatro maneras de entrar arriba y,
debajo, las revisiones que ya abriste antes.

![La pantalla de inicio](images/home-light.png#only-light)
![La pantalla de inicio](images/home-dark.png#only-dark)

El historial recuerda de dónde vino cada revisión y si la enviaste. Las
filas de solicitudes que ya se fusionaron o cerraron se podan, y con
ellas los worktrees que prchum había creado.

## La cola de revisión

⇧⌘L le pregunta a tu forja qué solicitudes te esperan.

![La cola de revisión](images/review-queue-light.png#only-light)
![La cola de revisión](images/review-queue-dark.png#only-dark)

El selector de arriba elige el filtro. Los filtros con nombre salen del
mapa `list_filters` de tu configuración, el filtro por defecto corre
cuando no eliges ninguno, y **Custom…** acepta un filtro escrito en el
momento para el resto de la sesión.

![El selector de filtros](images/queue-filters-light.png#only-light)
![El selector de filtros](images/queue-filters-dark.png#only-dark)

Return, o un doble clic, abre la solicitud resaltada.

## Leer un diff

La ventana de revisión es la barra lateral de archivos cambiados, la
barra de herramientas y el diff.

![Una ventana de revisión](images/review-window-light.png#only-light)
![Una ventana de revisión](images/review-window-dark.png#only-dark)

La barra lateral cuenta adiciones y borrados por archivo, y marca los
archivos que llevan comentarios. ⌘↓ y ⌘↑ recorren los cambios, ⌥⌘↓ y
⌥⌘↑ los hunks, ⇧⌘↓ y ⇧⌘↑ los archivos. El coloreado de sintaxis hace una
pasada de tree-sitter por cada lado de cada hunk, así que una
construcción de varias líneas colorea bien tanto en el texto viejo como
en el nuevo.

Abrir un pull request se ve igual: la barra de título nombra la
solicitud en lugar de la comparación.

![Un pull request](images/pull-request-light.png#only-light)
![Un pull request](images/pull-request-dark.png#only-dark)

⌘I muestra la descripción de la solicitud, renderizada como Markdown,
con la rama a la que se fusiona y un botón para abrirla en el navegador.

![Información del pull request](images/pr-info-light.png#only-light)
![Información del pull request](images/pr-info-dark.png#only-dark)

## Vista dividida

⌥⌘T pone los dos lados en paneles paralelos. El panel donde está el
cursor decide a qué lado apunta un comentario nuevo.

![Vista dividida](images/split-view-light.png#only-light)
![Vista dividida](images/split-view-dark.png#only-dark)

## Contexto de archivo completo

Un diff muestra tres líneas alrededor de cada cambio, que a menudo son
tres líneas de menos.

![Solo los hunks](images/hunk-view-light.png#only-light)
![Solo los hunks](images/hunk-view-dark.png#only-dark)

⌥⌘C trae el archivo entero y vuelve a colocar los hunks dentro, para que
leas el cambio donde vive. El código fuera del diff también se colorea,
y la descarga ocurre fuera del hilo principal: la ventana sigue viva
mientras llega.

![Contexto de archivo completo](images/context-view-light.png#only-light)
![Contexto de archivo completo](images/context-view-dark.png#only-dark)

## Comentar

⌘↩ comenta la línea bajo el cursor, o la selección.

![El compositor de comentarios](images/comment-composer-light.png#only-light)
![El compositor de comentarios](images/comment-composer-dark.png#only-dark)

Una selección de varias líneas se vuelve un comentario de rango, tal
como lo entiende la forja.

![Un comentario de varias líneas](images/multiline-comment-light.png#only-light)
![Un comentario de varias líneas](images/multiline-comment-dark.png#only-dark)

Los comentarios no están atados a una fila en pantalla. Se anclan a una
ubicación semántica —archivo, lado, rango de líneas y un ancla corta de
contexto con un hash del contenido de la línea—, y por eso un borrador
sobrevive a que la rama se mueva bajo él.

Los borradores y los hilos que ya están en la solicitud aparecen en
línea, enmarcados y con su Markdown renderizado.

![Un hilo de comentarios](images/comment-light.png#only-light)
![Un hilo de comentarios](images/comment-dark.png#only-dark)

⌘R responde dentro de un hilo, ⌘E edita el borrador bajo el cursor, ⇧⌘X
descarta uno (se guarda local, nunca se envía) y ⌘⌫ lo borra.

## El navegador de revisión

⌘L lista todos los borradores e hilos de la revisión; Return salta al
que elijas.

![El navegador de revisión](images/navigator-light.png#only-light)
![El navegador de revisión](images/navigator-dark.png#only-dark)

## Enviar

⇧⌘↩ abre la hoja de envío: cuántos comentarios y respuestas están por
salir, una caja para el resumen de la revisión, y el evento —comentar,
aprobar o pedir cambios—. ⌥⌘A y ⌥⌘R abren la misma hoja con aprobar o
pedir cambios ya elegido.

Los comentarios que la forja acepta se quitan de tus borradores locales
a medida que llegan, así que un envío que falla a mitad de camino se
puede reintentar sin publicar nada dos veces. Los borradores huérfanos
—aquellos cuyo código ya no está— nunca se envían, y la hoja te lo dice
antes de que te decidas.

## Ajustes

⌘, guarda lo que vale la pena cambiar.

![Ajustes](images/settings-light.png#only-light)
![Ajustes](images/settings-dark.png#only-dark)

Apariencia y tema; el nombre al que se atribuyen tus borradores; los
filtros de descubrimiento, el por defecto y los que tienen nombre; la
plantilla del editor; y el mapa de repositorios de la forja a clones
locales que usa **Edit File Locally** (⌃⌘E) para sacar la rama y abrir
el archivo donde está tu cursor.

Todo esto se escribe en `config.json`, que sigue siendo editable a mano:
las claves desconocidas sobreviven cada guardado, y un archivo que
prchum no puede interpretar nunca se sobrescribe.
