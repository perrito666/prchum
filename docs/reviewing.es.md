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
| ⇧⌥⌘↓ / ⇧⌥⌘↑ | archivo sin revisar siguiente / anterior |
| ⌥⌘V | marcar el archivo actual como revisado ↔ desmarcarlo |
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

## Archivos revisados

⌥⌘V marca como revisado el archivo en pantalla; la barra lateral le pone
una marca delante, atenúa su nombre y cuenta el progreso arriba — «3 de
12 revisados». Hacer clic en la marca hace lo mismo. ⇧⌥⌘↓ va al
siguiente archivo sin revisar, dando la vuelta al final de la lista, y
⇧⌥⌘↑ al anterior; cuando todos están marcados, lo dice.

Una marca pertenece a los cambios sobre los que se hizo. Si el diff de
un archivo cambia — un push nuevo, un árbol de trabajo editado — la
marca caduca y el archivo vuelve a contar como sin revisar, igual que el
"Viewed" de GitHub. Un rebase que solo mueve números de línea conserva
la marca.

Las marcas son solo tuyas: se guardan con tus borradores y nunca se
envían a la forja. Un documento de intercambio tampoco las lleva. El
shell de Linux todavía no tiene marcas.

## Un commit a la vez

La ventana de un pull request tiene un selector de commits al principio
de la barra: **All changes**, y después cada commit del pedido, del más
antiguo al más reciente, como `abc1234  título`. Elegir un commit
reemplaza la revisión, en la misma ventana, por los cambios de ese
commit respecto de su padre; elegir **All changes** vuelve atrás. El
título muestra `owner/repo#N @ abc1234` mientras estás en un commit.
(Por ahora en macOS; el shell de Linux todavía no tiene el selector.)

| Por defecto | Acción |
| --- | --- |
| ⌃⌘C | revisar un commit: la misma lista como menú, flechas y Retorno |
| ⌃⌘↓ / ⌃⌘↑ | commit siguiente / anterior (All changes va primero) |

Cada commit es una revisión propia. Sus borradores se guardan aparte de
los del pedido entero y de los de los otros commits, y vuelven cuando
eliges ese commit otra vez; el selector cuenta los borradores que
esperan en cada uno.

Los comentarios escritos sobre un commit se envían al pull request
**fijados a ese commit**, como los publica la vista de un solo commit
de la propia forja (ver [Forjas](forges.md)). Si la forja rechaza uno,
se muestra el error y los borradores se quedan para reintentar. Los
hilos de revisión existentes no se dibujan en línea sobre un commit —
sus posiciones pertenecen al diff del pedido entero—, pero la
conversación está ahí como siempre.

GitHub lista como mucho 250 commits de un pull request. En uno más
largo el selector lo avisa debajo de la lista; los commits posteriores
se pueden revisar igual en **All changes**.

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
| ⌥↩ | abrir la conversación del cursor en su propio lector |
| ⇧⌘T | desplegar ↔ plegar el hilo resuelto del cursor |
| ⌃⌘E | editar el archivo actual localmente (véase abajo) |

Una selección debe caer en un solo lado, al estilo de GitHub: un bloque
de cambios se ancla a la DERECHA (los borrados simplemente no forman
parte de ese lado) y una selección de solo borrados se ancla a la
IZQUIERDA. Los borradores se marcan con `●` en el margen, con la nota en
línea; los hilos existentes del servidor, con `◆`.

### Hilos desactualizados y resueltos

Un hilo está **desactualizado** (*outdated*) cuando el código sobre el
que se escribió ha cambiado desde entonces, de modo que ya no cae en
ninguna línea del diff. En lugar de dibujarlo junto al código que hoy
tenga ese número de línea, prchum lista los hilos desactualizados al
principio de su archivo, uno por línea — autor, la línea en la que
estaba, el comienzo del comentario. **Read…** (o ⌥↩ sobre esa línea)
abre la conversación completa en el lector, donde todavía se puede
responder.

Un hilo **resuelto** (*resolved*) que sigue en una línea se pliega a
una sola línea debajo de ella. **Expand**, o ⇧⌘T con el cursor encima,
lo muestra completo; **Collapse** o ⇧⌘T de nuevo lo vuelve a plegar.
⌥↩ lo abre en el lector sin desplegarlo.

El navegador de la revisión (⌘L) lista ambos, marcados como *outdated*
o *resolved*; Retorno salta a un hilo que tiene línea y abre el que no
la tiene. El contador `◆` de la barra lateral los incluye.

Resolver y reabrir hilos queda en manos del servidor. GitHub informa de
la resolución solo a través de su API GraphQL; si esa consulta falla,
los hilos se muestran completos en lugar de impedir que se abra la
revisión.

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

## Desde la terminal

Prchum está pensado para alcanzarse como se alcanza `git diff`, así que
acepta los mismos tipos de argumento:

```sh
prchum                  # lo que mostraría git diff
prchum --staged         # lo que mostraría git diff --staged
prchum main             # esta rama contra main
prchum v1..v2           # un rango
prchum cambio.diff      # un parche o documento de intercambio
prchum 418              # la solicitud 418 del origin de este repositorio
prchum owner/repo#418   # un repositorio explícito
```

`git prchum` hace lo mismo, porque git trata cualquier `git-*` del PATH
como un subcomando, y se ejecuta desde la raíz del repositorio, así que
significa lo mismo en un subdirectorio que en la raíz.

Para que sea el comando al que recurres, dale a git un alias:

```sh
git config --global alias.d '!git prchum'
```

Entonces `git d` abre lo que `git diff` habría impreso, y `git d main`
compara contra una rama.

Ambos comandos tienen página de manual: `man prchum`, `man git-prchum`,
y `git prchum --help`, que git responde desde esa misma página.

En macOS el comando se instala desde **Prchum → Install Command-Line
Tool…**, o con `make install-cli` desde un checkout. En Linux lo
instalan los paquetes.

!!! note "Antes se llamaba `pr`"

    Lo cual era un error: `pr` es el paginador de POSIX, tiene página de
    manual, y `/usr/local/bin` va antes que `/usr/bin` en el PATH por
    defecto, así que instalarlo ahí tapaba en silencio una herramienta
    estándar. Si tienes el antiguo, `rm /usr/local/bin/pr` lo deja como
    estaba.
