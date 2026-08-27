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

Cuando sale bien recibes una notificación, no un diálogo: no hay nada
que decidir, así que nada se interpone entre tú y lo siguiente. Un envío
que solo llegó a medias sí pregunta, porque eso es algo que tienes que
ver.

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

## En Linux

La misma revisión, en la interfaz GTK. El núcleo de prchum es una sola
biblioteca portable; lo que cambia entre plataformas es la presentación,
y cambia a propósito: esta es una aplicación de GNOME, no una de Mac
disfrazada.

![La ventana de revisión en Linux](images/linux-review-light.png#only-light)
![La ventana de revisión en Linux](images/linux-review-dark.png#only-dark)

Las filas son idénticas porque las decide el núcleo: los mismos
archivos, los mismos marcadores y números de línea, los mismos colores
de tree-sitter salidos de la misma tabla de estilos. Lo que cambia es
todo lo que las rodea: una barra de cabecera de libadwaita con el título
y el archivo, los controles de ventana propios de GNOME y sus colores de
acento en los contadores de la barra lateral. Sigue el ajuste claro u
oscuro del escritorio, como se ve arriba.

Comentar funciona como en macOS, porque lo hace el mismo núcleo:
**Ctrl+Return** abre el compositor en la línea bajo el cursor,
**Ctrl+E** edita un borrador, **Ctrl+Supr** lo borra y
**Ctrl+Mayús+X** lo descarta. Los borradores aparecen en línea bajo
la línea a la que pertenecen y sobreviven al cierre de la ventana: se
escriben junto a la configuración, en `~/.local/share/prchum`, según
la disposición XDG y no la de macOS.

Los atajos cambian, y a propósito. Las acciones tienen el mismo nombre
en ambas plataformas y las mismas entradas en el mapa `keys`, pero quien
usa GNOME pulsa Ctrl donde quien usa Mac pulsa Command, así que los
valores por defecto son de Ctrl: **Ctrl+↑/↓** recorre los cambios,
**Ctrl+Mayús+↑/↓** los archivos.

El resto también está. **Ctrl+Alt+C** vuelve a colocar los hunks dentro
del archivo entero; **Ctrl+Mayús+T** pone los dos lados en paneles
paralelos que se desplazan juntos; **Ctrl+Mayús+L** le pregunta a la
forja qué espera y abre lo que elijas; **Ctrl+,** guarda los ajustes, en
el mismo `config.json` que escribe la app de macOS; y
**Ctrl+Mayús+Return** envía, con la misma seguridad ante reintentos: lo
que la forja aceptó sale de tus borradores aunque falle un paso
posterior.

**Ctrl+R** responde a un hilo que la solicitud ya tiene, o a un borrador
tuyo, y **Ctrl+Mayús+P** abre la conversación: los comentarios que
pertenecen a la solicitud y no a una línea.

## Compartir un enlace

Hay dos cosas que vale la pena mandarle a alguien: dónde estás mirando
en el código, y a qué conversación te refieres.

**Ctrl+Mayús+C** (⇧⌘C en macOS) copia un enlace permanente a la línea
bajo el cursor. Apunta al archivo tal como está en el commit de cabecera
de la solicitud, no a la pestaña de archivos: las anclas que cada forja
pone en un diff se escriben distinto y se mueven cuando la solicitud se
actualiza, mientras que un blob en un commit concreto le funciona a
quien se lo mandes, y sigue funcionando.

**Ctrl+Mayús+K** (⇧⌘K) copia el enlace propio de la forja al hilo bajo
el cursor, para cuando te refieres a *esa conversación* y no a esa línea.

Con **Alt** (Opción en macOS) cada uno abre el enlace en lugar de
copiarlo: Ctrl+Alt+Mayús+C y Ctrl+Alt+Mayús+K. Alt significa «abrir en
vez de copiar» en todos los casos, y por eso el par del hilo va en la K
y no comparte la C.

Los cuatro están también en el **menú contextual** del diff, así que no
hay que recordarlos. En macOS el menú muestra uno de cada par y cambia
copiar por abrir mientras mantienes Opción, como funcionan allí los
elementos alternativos; en Linux se listan los cuatro. En ambos casos el
cursor se mueve primero a donde hiciste clic, de modo que la acción es
sobre la línea que señalaste.

Ambos necesitan un pull request detrás de la revisión: un parche o una
comparación local no tienen a dónde apuntar, y lo dicen en lugar de
copiar algo inútil.
