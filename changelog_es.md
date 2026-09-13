Registro de cambios

Versión 0.5.0 - 12 de septiembre de 2026

Audiodescripción, Herramientas y conversión por lotes

1. Se han llevado a macOS las últimas mejoras del motor de audiodescripción de Windows, con un puente de Gemini más resistente, mejor gestión de segmentos de vídeo problemáticos y controles más fiables durante el análisis y la reexportación.

2. En “Crear audiodescripción con IA” ahora se puede elegir entre “Usar mi clave API de Gemini” y “Usar Sonarpad AI”. Las dos credenciales se guardan por separado, por lo que cambiar de modo no elimina ni la clave personal ni el código de Sonarpad AI.

3. Al usar Sonarpad AI, el crédito actual se muestra en un campo de solo lectura, junto con los controles para mostrar el código y solicitar uno nuevo.

4. Si el archivo contiene varias pistas de audio, Sonarpad pregunta qué pista debe analizar antes de crear la audiodescripción. La pista elegida se guarda en el proyecto y se reutiliza en las operaciones posteriores.

5. Se añadió una opción para reconocer textos importantes que aparecen en pantalla y tenerlos en cuenta al generar las descripciones.

6. Ahora es posible crear opcionalmente un vídeo final que incluya la audiodescripción, además de la salida de audio normal. También se mejoró la gestión de contenedores y marcas de tiempo cuando es necesario utilizar un formato alternativo.

7. Se mejoró el ducking del audio original: la reducción y recuperación del volumen alrededor de la narración son ahora más graduales, con pre-duck y liberación más suaves.

8. Se añadió “Reanalizar segmento” al editor de proyectos de audiodescripción. Actúa sobre la descripción seleccionada y utiliza automáticamente el último modo global de IA elegido, igual que en Windows.

9. El editor de proyectos puede mantener pendientes los cambios de varias descripciones al mismo tiempo. Los borradores se conservan al cambiar de segmento o al buscar; al pulsar “Aplicar”, todos se validan y se aplican juntos. Si una sola descripción no cabe en el silencio disponible, no se aplica ningún cambio y Sonarpad vuelve a la descripción que debe corregirse.

10. Se corrigió la ruta de guardado al crear una segunda audiodescripción sin cerrar la ventana: al seleccionar un nuevo archivo de origen se genera ahora una ruta para ese archivo, en lugar de conservar la anterior.

11. Se añadió la opción “Agrupar el menú Herramientas por categoría”. Cuando está activa, Herramientas se organiza en “Lectura y contenido”, “Multimedia” y “Utilidades”; al desactivarla vuelve el menú plano.

12. En la enciclopedia Treccani se eliminó de la interfaz accesible el control de resultados vacío que aparecía antes de realizar una búsqueda. Ahora solo se muestra cuando existen resultados seleccionables.

13. Se añadió “Convertir carpeta…” en Herramientas > Multimedia. Es posible convertir por lotes una carpeta completa usando los mismos formatos y parámetros de “Convertir multimedia”, por ejemplo muchos archivos WMA a MP3 en una sola operación.

14. La conversión de carpetas muestra el progreso archivo por archivo, propone una subcarpeta “Convertidos”, conserva los nombres base, resume los errores y evita sobrescrituras accidentales y colisiones entre nombres de destino.

15. Se ha añadido un ajuste, activado de forma predeterminada, que al avanzar o retroceder por un contenido multimedia anuncia tanto la posición actual como la duración total en un formato natural, por ejemplo «1 minuto y 10 segundos de 1 hora, 10 minutos y 10 segundos». Al desactivarlo, Sonarpad sigue anunciando únicamente la posición actual, como en las versiones anteriores.

16. Durante la reproducción, Option+I anuncia únicamente la duración total del contenido en formato natural. El atajo funciona independientemente del ajuste que añade la duración total a los anuncios al avanzar o retroceder; en las emisiones en directo se anuncia que el contenido está en directo.

17. En Crear audiodescripción con IA, el motor y la voz ya no ocupan la ventana principal. El nuevo botón “Ajustar voz” abre una ventana dedicada con motor, voz, velocidad y volumen, además de la prueba de voz; las opciones se guardan para las audiodescripciones. Si nunca se usa esta ventana, la velocidad y el volumen siguen heredando la configuración general como antes.

18. “Reproducir contenido en streaming” muestra ahora la duración de cada vídeo junto al título. En “Convertir carpeta”, las etiquetas son más claras con “Elegir carpeta para convertir” y “Carpeta de destino”; durante la conversión, “Interrumpir conversión” termina inmediatamente el proceso FFmpeg activo, elimina el archivo parcial actual y evita iniciar los archivos siguientes.

19. Se añadió una protección de respaldo para fuentes multicanal problemáticas (por ejemplo 5.1, 6.1 o 7.1): la canalización normal permanece sin cambios y se usa como antes; solo si el WAV interno no se puede leer, tiene un formato inesperado o contiene tramas PCM desalineadas, Sonarpad regenera automáticamente ese paso en estéreo a 48 kHz y vuelve a intentarlo, evitando errores de finalización sin afectar a los archivos que ya funcionan.

20. Al iniciar una conversión de un archivo o de una carpeta, VoiceOver anuncia ahora «Conversión iniciada», para confirmar inmediatamente que el proceso ha comenzado sin tener que desplazarse hasta el indicador de progreso.

21. Mejorada la fiabilidad de las fuentes RSS: si el feed original de un medio falla o no devuelve artículos, Sonarpad prueba automáticamente un feed de Google News limitado al sitio del mismo medio y en el idioma de noticias seleccionado. El feed original permanece guardado y conserva la prioridad; el fallback también cubre Il Giornale y los principales hosts técnicos de feeds.

22. Corregido un problema en macOS por el que, al cerrar un documento modificado y elegir «No guardar», la solicitud de guardado podía aparecer una segunda vez. Sonarpad ahora recuerda la confirmación para el evento de cierre actual y pregunta una sola vez.


23. “Fuentes de la comunidad” muestra ahora siempre todas las fuentes disponibles para el idioma seleccionado, incluidas las que ya están en la biblioteca. Las fuentes ya importadas se marcan como “Ya importada”; al seleccionar una se pregunta si se desea sustituirla. La sustitución actualiza la misma entrada sin crear duplicados y conserva la carpeta en la que el usuario la hubiera organizado.

24. Se añadió “Ir a la fecha” a Podcasts y RaiPlay Sound, siguiendo el comportamiento de la versión móvil. En Podcasts el comando aparece al principio del submenú solo cuando el feed contiene fechas reales; al elegir una fecha se muestra la lista completa de episodios de ese día, incluidos los que quedan fuera de los primeros 30 del menú. RaiPlay Sound muestra un botón contextual “Ir a la fecha” cuando hay contenidos con fecha. Los selectores muestran directamente solo las fechas disponibles, sin etiquetas redundantes, para una navegación más limpia con VoiceOver.


Versión 0.4.0 - 3 de septiembre de 2026

Audiodescripción con IA — nueva función principal

- Se añadió “Crear audiodescripción con IA” directamente al menú Herramientas. Sonarpad analiza el audio para encontrar espacios sin diálogo, genera las descripciones con Gemini y utiliza los motores de voz ya disponibles, evitando hablar sobre los diálogos.

- Se mejoró la sincronización entre lo que ocurre en el vídeo y las descripciones, con controles automáticos sobre los tiempos generados por Gemini.

- “Activar pausas extendidas” está desmarcada de forma predeterminada. Puede activarse en contenidos con mucho diálogo o poco espacio disponible para permitir descripciones más largas.

- Sonarpad puede intentar reconocer a los personajes y usar sus nombres. Los catálogos de personajes pueden mantenerse entre episodios de una serie para mejorar la continuidad.

- Es posible guardar el proyecto, modificar después las descripciones y volver a exportar sin tener que regenerarlo todo con Gemini.

- Si el proceso se interrumpe, Sonarpad conserva el progreso y permite continuar la audiodescripción. Si se agota la cuota de Gemini, se puede esperar, cambiar de modelo o detenerse sin perder el trabajo completado.

- La ventana permite elegir idioma, nivel de detalle, modelo Gemini, motor y voz, y recuerda las preferencias utilizadas. El módulo está disponible en los idiomas compatibles con Sonarpad para Mac.

- Durante la generación, la interfaz muestra el progreso, el estado actual y Cancelar; al terminar, el MP3 puede abrirse directamente en el reproductor interno.

- Mejorada la compatibilidad con vídeos MKV: Sonarpad gestiona de forma más fiable las marcas de tiempo irregulares o ausentes y, cuando es posible, omite los paquetes dañados sin detener la audiodescripción.

- Corregido un problema que podía hacer fallar la exportación final a MP3 con vídeos que contienen audio multicanal, como Dolby 5.1. Sonarpad convierte automáticamente el audio multicanal a estéreo cuando es necesario para codificar MP3.

- Cuando un vídeo contiene varias pistas de audio, Sonarpad pregunta qué pista se debe utilizar antes de empezar. El cuadro combinado accesible se puede cambiar con las flechas; Aceptar inicia la audiodescripción con la pista seleccionada y Cancelar cierra la ventana y devuelve el foco al editor de Sonarpad.

- Se añadió la casilla “Mostrar clave API” junto a la clave de Gemini. La clave permanece oculta de forma predeterminada y solo se muestra temporalmente mientras la casilla está activada; al volver a abrir la ventana queda oculta de nuevo.

YouTube y streaming

- Se ha mejorado notablemente la experiencia de YouTube, acelerando la búsqueda y la navegación y restableciendo su correcto funcionamiento.

- Las opciones de calidad de vídeo están ahora traducidas: en lugar del valor técnico “best”, Sonarpad muestra una etiqueta comprensible en el idioma de la interfaz.

- Sonarpad recuerda el último formato elegido en Guardar multimedia. Por ejemplo, si se selecciona MP4, MP4 seguirá preseleccionado la próxima vez que se abra el diálogo.


Agradecimientos

- Un agradecimiento especial a Leonardo Graziano y Tiziano Ferraro, que han probado a fondo la función de Audiodescripción con IA y Sonarpad en general, contribuyendo de forma muy valiosa a su mejora.

- Un gran agradecimiento también al grupo Tecnologia Accessibile por su apoyo, sus pruebas y sus sugerencias.

Versión 0.3.1 - 16 de julio de 2026

- Se ha corregido un problema que impedía iniciar Sonarpad cuando el menú Radio contenía favoritos, debido a identificadores de menú no válidos en wxWidgets.

- Sonarpad está ahora disponible también en francés, español, portugués, checo y polaco, además de italiano e inglés.

- Se ha añadido una configuración independiente para el Idioma de las noticias. Esta opción es independiente del idioma de la interfaz y permite que Sonarpad utilice fuentes y servicios adaptados al idioma seleccionado.

- Se ha añadido la función Tiempo, que permite buscar una ciudad y consultar las condiciones actuales, la temperatura, las precipitaciones, el viento y la humedad, así como las previsiones para hoy, mañana u otro día.

- Se ha añadido la sección Películas en cartelera, con las películas que se proyectan actualmente, los próximos estrenos, resúmenes de la trama, fechas de estreno y, cuando está disponible, enlaces a los tráileres.

- Se ha añadido un calendario accesible al menú Herramientas. Es posible seleccionar cualquier fecha, consultar las festividades, el santo y la frase del día, crear recordatorios y añadir citas directamente al Calendario de macOS.

- Se ha añadido la función Buscar rutas, que permite calcular rutas a pie, en bicicleta, en automóvil o accesibles para sillas de ruedas. Se puede elegir la ruta más rápida o la más corta y consultar la distancia, la duración estimada y las indicaciones detalladas.

- Se ha añadido la función Convertir medios, compatible con la conversión de archivos de audio y vídeo a varios formatos, entre ellos MP3, M4A, M4B, MP4, AVI, MOV, Opus, OGG, FLAC, WAV y AIFF. También es posible crear un vídeo a partir de un archivo de audio y una imagen.

- Se ha añadido el Diccionario de voz. Es posible definir palabras o expresiones que el sintetizador de voz debe sustituir durante la lectura, para corregir pronunciaciones, abreviaturas y nombres particulares.

- Se ha ampliado la sección Artículos con los comandos Artículos recientes y Compartir, que permiten volver rápidamente a los últimos contenidos leídos y compartir artículos mediante los servicios disponibles en macOS.

- Se han añadido al menú Artículos las opciones Añadir una fuente de noticias a la comunidad de Sonarpad y Fuentes de noticias de la comunidad de Sonarpad. Es posible enviar un canal RSS o un sitio de noticias e importar fuentes compartidas por otros usuarios. Las fuentes se añaden y se muestran según el Idioma de las noticias seleccionado.

- Se ha mejorado la gestión de las fuentes de noticias. Al cambiar el Idioma de las noticias se cargan ahora las fuentes predeterminadas adecuadas sin eliminar las fuentes añadidas personalmente por el usuario.

- Se ha ampliado la búsqueda de radios con la exploración por idioma, país y ciudad, incluidos los nombres completos y localizados de los países.

- Se ha añadido la posibilidad de enviar una emisora de radio a la comunidad de Sonarpad indicando su nombre, dirección de transmisión, idioma y género.

- Se han añadido la grabación de radio y la grabación programada. Estas acciones están disponibles tanto en los resultados de búsqueda como en los favoritos, y las grabaciones se guardan directamente como archivos MP3. Después de abrir una emisora de radio, también se puede iniciar la grabación pulsando la letra R.

- Se ha añadido al menú Archivo una lista de los documentos de texto abiertos recientemente, para poder volver a abrirlos con mayor rapidez.

- Se ha añadido el modo Solo lectura, útil para consultar un documento sin modificarlo accidentalmente.

- Se ha añadido Contenido del libro para los archivos EPUB que incluyen un índice. Es posible seleccionar un capítulo e ir directamente a él.

- Se ha añadido la opción de elegir entre las voces Microsoft de alta calidad y las voces del sistema de macOS.

- Se ha añadido una opción para ignorar durante la lectura las pausas causadas por líneas vacías.

- Se ha añadido una configuración para elegir cuántos segundos avanzar o retroceder durante la reproducción multimedia.

- Se ha mejorado la accesibilidad de las ventanas, los menús y los controles, con una gestión más coherente del foco, Enter, Escape y los atajos de teclado.

- Se ha mejorado la localización de mensajes, botones y cuadros de confirmación en todos los idiomas compatibles.

- Se corrigió un problema por el que los archivos multimedia no se mostraban en la ventana de vídeo del reproductor.

- Se han corregido numerosos problemas que afectaban a la estabilidad, la reproducción multimedia, las grabaciones de radio programadas, la gestión de fuentes y la compilación en macOS.

- Un agradecimiento especial a Leonardo Graziano, Luca Maianti y al grupo italiano Tecnologia Accessibile por su apoyo continuo y sus constantes pruebas beta.

Versión 0.2.9 - 1 de mayo de 2026
- Se ampliaron las funciones de YouTube también a los Mac Intel y Catalina.
- Se aceleró enormemente la búsqueda en YouTube.
- Se mejoró la gestión de los resultados de YouTube, colocando canales y listas de reproducción al principio.
- Se añadió la posibilidad de añadir y eliminar canales y listas de reproducción de los favoritos.
- Se añadió el botón Vista previa de la voz en las opciones.
- Se añadió el botón Seleccionar todo al eliminar fuentes.
- Se añadió una barra de progreso para la búsqueda en Wikipedia.
- Se añadió el canal de TV Videolina.
- Se trasladaron las opciones de las funciones adicionales al menú Herramientas, para alinear Sonarpad con la versión para Windows.
- Se corrigió el comportamiento por el que a veces no se mostraban los programas que estaban en emisión en la TV.
- Se añadieron numerosos canales de TV, organizando la ventana en categorías para facilitar la consulta. También se añadió un campo de búsqueda que muestra los resultados del canal deseado.

Versión 0.2.8 - 29 de abril de 2026
- Se añadió el menú Herramientas con dos nuevas opciones: Buscar e importar desde Wikipedia y Reproducir audio en streaming.
- Buscar e importar desde Wikipedia permite buscar e importar artículos, leerlos y guardarlos como audiolibros.
- Reproducir audio en streaming permite reproducir contenidos en streaming, por ejemplo desde YouTube.
- En el cuadro de búsqueda de streaming se puede escribir cualquier contenido: el programa lo buscará y también podrá abrir canales y listas de reproducción.
- La búsqueda de YouTube no está habilitada en los Mac Intel por motivos de incompatibilidad.
- Agradecimiento especial a Leonardo Graziano por su apoyo continuo.
- En las radios se añadió un botón para ir directamente a la página seleccionada de los resultados, sin tener que usar siempre Ir a la página siguiente.
- Se extendió el marcador automático también a los archivos de texto.
- Se corrigió un problema por el que a veces las audiodescripciones no se guardaban por problemas de tiempo de espera.
- Se añadió la posibilidad de configurar TV favoritas.
- En la lista de canales de TV se añadió la indicación del programa en emisión.
- Se incorporó una guía TV completa, consultable desde el día anterior hasta cinco días después de la fecha actual.

Versión 0.2.7 - 28 de abril de 2026
- Se mejoró el soporte para archivos con diacríticos y codificaciones distintas de UTF-8, incluido el soporte para caracteres chinos y otros idiomas internacionales.
- Se corrigió el problema por el que la coma, escrita en un campo de texto, abría erróneamente las opciones.
- Se mejoró la rapidez de lectura: ahora también los artículos largos se leen más rápido y se eliminó la pausa después de los párrafos.
- Se añadió la posibilidad de abrir con Sonarpad archivos JPG y formatos similares, para poder realizar OCR también en artículos enviados como imágenes o fotografías.
- Se añadió la posibilidad de establecer Sonarpad como programa predeterminado.
- A partir de ahora Sonarpad puede abrir no solo archivos de texto, sino también archivos de audio y vídeo, usando el reproductor MPV.
- Se añadió en las opciones la función de marcador automático: si se cierra un archivo, un podcast o cualquier contenido multimedia, se volverá a abrir exactamente desde la posición en que se dejó.
- Las radios ya no se abren en Safari, sino que se reproducen directamente mediante el reproductor de Sonarpad.
- Desde esta versión la app está firmada y ya no requiere ninguna autorización por parte del usuario, lo que hace la instalación más sencilla.
- Se añadió una actualización automática del programa, que comprueba, descarga y actualiza Sonarpad automáticamente.
- Se incorporaron los módulos adicionales de RaiPlay, Audiodescripciones Rai, RaiPlay Sound y canales TV. Para utilizarlos será necesario solicitar un código al autor.
- Para obtener el código, siga el procedimiento indicado por el programa y envíe el correo generado, asegurándose de que esté realmente presente en el correo enviado. Si el procedimiento se realiza correctamente, el código se recibirá en aproximadamente un minuto.
- El código debe introducirse abriendo las opciones con Command + , y desplazándose con VO + flecha derecha hasta el campo Código Sonarpad para funciones adicionales.
- Nota: si al abrir una función adicional, por ejemplo RaiPlay, aparece un error, probablemente significa que el código no se copió por completo.
- En los módulos Rai se añadieron la búsqueda y la consulta de contenidos, que se reproducen mediante el reproductor de Sonarpad.

Versión 0.2.6
- Se corrigió un error de wx/macOS por el que al iniciar podía aparecer un error y se estabilizaron los menús relacionados.
- Se corrigió el atajo Cmd+, para el menú Opciones incluso cuando el foco está en el editor o en otros controles.
- Al guardar un audiolibro, el foco se coloca ahora correctamente en el campo de texto y los nombres de archivo con punto ya no se recortan.
- Se añadió soporte para OPML de Lire con división en carpetas: las carpetas se abren como submenús y las fuentes individuales en una ventana dedicada.
- La reordenación de las fuentes de artículos ahora gestiona el nuevo sistema de carpetas con los botones Abrir carpeta, Carpeta principal, Mover a carpeta y Mover fuera de las carpetas.

Versión 0.2.5
- Nuevas ventanas de guardado personalizadas para texto y audiolibros en macOS.
- Los campos de nombre de archivo aceptan ahora correctamente Cmd+V, Cmd+A y los demás comandos de edición.
- El programa recuerda la última carpeta y el último formato usados para guardar texto y audiolibros.
- Se añadió el guardado de audiolibros también en formato M4A y WAV.
- Se añadió el menú Radio con búsqueda por idioma, añadir a favoritos, añadir manualmente una emisora y modificar y reordenar favoritos.
- Se mejoró la gestión de las fuentes de artículos añadidas como sitios: detección del feed desde la página y corrección del feed de comentarios.
- Se actualizó el flujo de publicación de macOS para incluir también el artefacto Catalina.

Versión 0.2.4
- Mejoras importantes del OCR de PDF en macOS con el paso a pdfium y alternativas más robustas.
- Se añadió la exportación M4B en macOS y se perfeccionó el guardado de texto.
- Se mejoró la gestión de las fuentes de artículos y la protección de la actualización cuando una fuente devuelve cero elementos.
- Se optimizó la síntesis Edge TTS con división en bloques y reintentos más fiables.
- Se añadió y refinó la canalización Catalina para compilación y empaquetado en macOS.

Versión 0.2.2
- Se mejoró la carga de PDF en macOS con información más clara y un cuadro final explícito.
- Ordenación alfabética de las fuentes de artículos.
- Reparaciones del texto PDF y mejoras generales de localización.

Versión 0.2.1
- Se estabilizaron los atajos y los menús de macOS para iniciar, pausar, detener y guardar.
- Se mejoró la apertura externa de episodios de podcast en macOS.
- Se corrigió la persistencia de las opciones en macOS.
- Se reforzaron los flujos de compilación Intel/macOS y la gestión de Xcode.

Versión 0.2.0
- Primera versión macOS de Sonarpad para Mac.
- Soporte de lectura de texto, artículos y podcasts con síntesis de voz.
- Soporte de OCR PDF en macOS, descarga de actualizaciones y paquetes DMG dedicados.
- Categorías de podcast jerárquicas y primeros atajos globales/macOS.
