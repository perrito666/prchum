# Prchum

Un cliente de revisión de código nativo para macOS. Revise un **archivo
de parche**, una **comparación git local**, un **pull request de
GitHub** o un **pull request de Forgejo** en la misma aplicación:
navegue el diff, deje comentarios borrador anclados a ubicaciones
semánticas y exporte sus notas como Markdown o como documento de
intercambio — o envíelas como una revisión real.

Prchum es la funcionalidad de
[leanreview](https://github.com/perrito666/leanreview) reconstruida
sobre la arquitectura de
[textchum](https://github.com/perrito666/textchum): un núcleo compilado
portable (Rust) detrás de una carcasa totalmente nativa (Swift +
AppKit), que se encuentran en una interfaz C. El núcleo es dueño del
diff y del estado de la revisión; la carcasa, de la plataforma.

Es un cliente de revisión, no un cliente git: los `git`, `gh` y `fj`
instalados se encargan de la semántica del repositorio y de la forja —
Prchum nunca guarda un token. Lo suyo es la navegación, el renderizado,
el estado de la revisión y los comentarios.

## La regla que todo respeta

Las filas renderizadas nunca son ubicaciones canónicas de comentarios.
Cada comentario se ancla a una **ubicación** semántica — ruta, lado
(LEFT/RIGHT), rango de líneas y el texto que lo rodea — y por eso
sobrevive a los cambios de disposición, al plegado, a alternar entre
vista unificada y dividida y, mediante una reubicación conservadora,
incluso a que la punta del pull request se mueva entre sesiones.

## Por dónde seguir

- [Primeros pasos](getting-started.md) — instalar, compilar, primera
  revisión.
- [La revisión](reviewing.md) — navegación, comentarios, envío.
- [Fuentes](sources.md) — parches, git, pull requests, el formato de
  intercambio.
- [Forjas](forges.md) — GitHub, Forgejo e instancias propias.
- [Configuración](configuration.md) — teclas, forjas, descubrimiento.
