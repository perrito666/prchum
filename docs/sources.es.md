# Fuentes

Prchum revisa cuatro clases de fuente en la misma ventana.

## Archivos de parche

Abra un `.diff`/`.patch` (File → Open…, arrastrándolo a la aplicación o
por línea de comandos). Los borradores se indexan por la ruta absoluta
del archivo, así que revisar el mismo archivo otra vez retoma las mismas
notas.

## Comparaciones git locales

File → Review Git Repository… elige un checkout y una comparación:

- **Árbol de trabajo contra HEAD** — lo que muestra `git diff`.
- **Índice contra HEAD** — lo que muestra `git diff --cached`.
- **Contra una referencia base** — `base...HEAD`, la comparación por
  merge-base.

Cada comparación conserva su propio borrador: las notas sobre lo
preparado nunca se mezclan con las notas contra `main`.

## Pull requests

Una URL, `owner/repo#N` o un número a secas (el repositorio se infiere
del origin del directorio actual). Prchum descarga el **diff canónico**
del servidor, así que las posiciones de los comentarios siempre
coinciden con lo que la forja muestra, más los hilos de revisión
existentes (marcadores `◆` — ⌘R responde) y los metadatos del pull
request (⌘I). Véase [Forjas](forges.md) para servidores y
autenticación.

Si la punta se mueve entre sesiones, los borradores guardados se
reanclan por su contexto capturado: una coincidencia exacta conserva su
sitio, una coincidencia única de texto sigue al código, y lo ambiguo
queda **huérfano** — conservado, marcado y nunca enviado. Adivinar
pondría una nota en la línea equivocada.

## El formato de intercambio

Prchum lee y escribe el formato `*.review.json` de leanreview (versión
1), detectado por contenido, nunca por nombre de archivo. Un LLM escribe
su revisión en un documento autocontenido; usted la tría en Prchum —
descartar, editar, responder — y cada cambio reescribe el archivo en su
sitio, de modo que al salir la conversación queda al día para la
siguiente ronda del modelo. Los documentos sin cambios viajan de ida y
vuelta byte a byte idénticos, así que ambos clientes interoperan sobre
los mismos archivos.
