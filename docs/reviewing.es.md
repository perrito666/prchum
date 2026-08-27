# La revisión

Cada operación es una acción con nombre: un elemento de menú con su
equivalente de teclado nativo de macOS, reasignable a través del mapa
`keys` de la [configuración](configuration.md). El ratón funciona en
todas partes, pero nada lo exige. La barra de la ventana lleva las
acciones comunes para los días de ratón, y ⇧⌘H vuelve a la pantalla de
inicio; las hojas se confirman con ⌘↩ (Retorno escribe una línea nueva
en el cuerpo).

## Navegación y vista

| Por defecto | Acción |
| --- | --- |
| ⌘↓ / ⌘↑ | cambio siguiente / anterior |
| ⌥⌘↓ / ⌥⌘↑ | hunk siguiente / anterior |
| ⇧⌘↓ / ⇧⌘↑ | archivo siguiente / anterior |
| ⌘F | buscar en el diff (la barra de búsqueda nativa) |
| ⌥⌘T | vista unificada ↔ dividida |
| ⌥⌘C | contexto completo: el archivo entero con los hunks superpuestos |
| ⌥⌘← | plegar / desplegar el hunk actual |
| ⇧⌥⌘← / ⇧⌥⌘→ | plegar / desplegar todos los hunks |
| ⌥⌘S | ciclar el coloreado: sintaxis + tintes → solo tintes → plano |
| ⌥⌘W | ajustar las líneas largas |
| ⌃⌘S | mostrar u ocultar la barra de archivos |

El coloreado sintáctico ejecuta una pasada de tree-sitter por lado y por
hunk, así que las construcciones multilínea se colorean bien en ambos
lados de un cambio. Se incluyen catorce lenguajes.

En la vista dividida los dos lados van en paneles paralelos; el panel
donde está el cursor decide a qué lado apunta un comentario.

## Comentarios

| Por defecto | Acción |
| --- | --- |
| ⌘↩ | comentar la línea del cursor o la selección |
| ⌘E | editar el borrador bajo el cursor |
| ⌘⌫ | borrar el borrador bajo el cursor |
| ⇧⌘X | descartar ↔ restaurar (se conserva; no se envía mientras esté descartado) |
| ⌥⌘↩ | sugerir un cambio: el código seleccionado prellenado en un bloque ```suggestion |
| ⌘R | responder — al hilo del servidor o a la conversación del borrador |
| ⌘L | el navegador de la revisión: cada borrador e hilo; Retorno salta |
| ⌃⌘E | editar el archivo actual localmente (véase abajo) |

Una selección debe caer en un solo lado, al estilo de GitHub: un bloque
de cambios se ancla a la DERECHA (los borrados simplemente no forman
parte de ese lado) y una selección de solo borrados se ancla a la
IZQUIERDA. Los borradores se marcan con `●` en el margen, con la nota en
línea; los hilos existentes del servidor, con `◆`.

Descartar no es borrar: el veredicto viaja con la revisión — es la
información que más necesita el otro lado de una conversación — pero un
comentario descartado nunca se envía.

## La edición local

⌃⌘E abre el archivo del cursor en su editor, dentro de una copia local
de la rama en revisión — en la línea del cursor cuando esa línea existe
en el archivo (un borrado lo abre sin línea).

La copia sale del clon que indique en la
[configuración](configuration.md): si la rama ya está activa allí —en el
propio clon o en un worktree suyo— se usa esa y no se toca; si no,
prchum crea un worktree propio junto a su estado, descargando la punta
de la solicitud cuando la rama todavía no es local. Solo se eliminan los
worktrees que prchum creó, y solo cuando la solicitud se fusionó, se
cerró o desapareció.

Una comparación git no necesita clon: ya es una copia de trabajo, así
que el archivo se abre ahí mismo.

## El envío

⇧⌘↩ abre la hoja de envío en una sesión de pull request; ⌥⌘A la abre
con **Approve** preseleccionado y ⌥⌘R con **Request changes** — la hoja
confirma igualmente. Muestra los conteos,
el selector de evento (Comment / Approve / Request changes), el resumen
y un aviso explícito por los comentarios huérfanos, que nunca se envían.
Nada se manda antes de esta confirmación.

El envío es seguro ante reintentos: la aplicación registra exactamente
lo que el servidor aceptó, incluso si un paso posterior falla, así que
un reintento manda solo lo pendiente — nunca un duplicado.

## La exportación

⇧⌘E escribe sus notas a un archivo: Markdown agrupado por archivo, o —
con un nombre `.json` — un documento de intercambio autocontenido
(véase [Fuentes](sources.md)) que incrusta el parche.
