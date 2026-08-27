# Primeros pasos

## Instalar

Descargue `Prchum.app` de las
[releases](https://github.com/perrito666/prchum/releases): está firmada
y notarizada, así que basta con descargar, descomprimir y hacer doble
clic; Apple Silicon, macOS 14+. En Linux, tome el tarball `prchum-gtk`
del mismo sitio; necesita GTK 4.12 y libadwaita 1.5 o posteriores. O
compile desde el código:

```sh
make run ARGS=cambio.diff   # compila núcleo + app y abre un parche
make app                    # un Prchum.app de doble clic en dist/
make check                  # lo que ejecuta CI: tests, smoke test, deriva de la cabecera
```

El comando de terminal `pr` se instala desde el menú de la aplicación
(Prchum → Install pr Command…) o con `make install-cli`.

Compilar requiere una cadena de herramientas de Rust y Xcode. Revisar
pull requests requiere el CLI de la forja instalado y autenticado:
[`gh`](https://cli.github.com) (`gh auth login`) para GitHub,
[`fj`](https://codeberg.org/forgejo-contrib/forgejo-cli) para Forgejo.
Revisar parches y comparaciones git locales no necesita más que `git`.

## Primera revisión

Al abrirse sin destino, la aplicación pregunta qué revisar: un pull
request, su cola de revisión, un archivo de parche o un repositorio git.
Cada puerta está también en el menú File, y todo funciona desde la línea
de comandos:

```sh
Prchum cambio.diff                           # un archivo de parche
Prchum 418                                   # el PR 418 del origin del repo actual
Prchum owner/repo#418                        # repositorio explícito
Prchum https://github.com/owner/repo/pull/418
```

Con el diff abierto:

1. **Navegue.** ⌘↓/⌘↑ saltan entre cambios, ⌥⌘↓/⌥⌘↑ entre hunks,
   ⇧⌘↓/⇧⌘↑ entre archivos — o haga clic en un archivo de la barra
   lateral.
2. **Comente.** Sitúe el cursor (o seleccione líneas) y pulse ⌘↩.
   Escriba la nota y pulse Comment. La línea recibe un marcador `●` y
   la nota aparece en línea debajo.
3. **Envíe o exporte.** En un pull request, ⇧⌘↩ abre la hoja de envío —
   nada se manda antes de confirmar. En cualquier fuente, ⇧⌘E exporta
   sus notas como Markdown (o como documento de intercambio si el
   nombre termina en `.json`).

Los borradores se guardan solos por fuente y se recargan la próxima vez
que abra la misma comparación.
