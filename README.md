# Mazetot

Pequeño juego de terror en primera persona hecho en Rust con [raylib](https://www.raylib.com/),
sobre un motor de raycasting propio (la misma técnica de Wolfenstein 3D): el mundo es un mapa 2D
de caracteres y la "tercera dimensión" se arma lanzando un rayo por cada columna de pantalla y
convirtiendo la distancia a la pared en la altura de esa columna. Todo se dibuja pixel por pixel
sobre un framebuffer propio en RAM; raylib solo se usa para abrir la ventana, leer teclado/mouse,
reproducir audio y subir ese framebuffer a la pantalla como textura.

Sobre ese motor hay un juego completo: un menú con 3 niveles, tótems que hay que destruir, un
enemigo que persigue con IA (visión + memoria de la última posición conocida + pathfinding),
lockers empotrados en las paredes para esconderse de él, una linterna como única fuente de luz
y audio dinámico (pasos, ambiente y zumbidos que cambian de volumen según la distancia).

## Video

<video src="MazetotDemo.mp4" controls width="720">
  Tu visor de Markdown no puede reproducir el video embebido — descárgalo o ábrelo directo:
  <a href="MazetotDemo.mp4">MazetotDemo.mp4</a>.
</video>

[▶ Ver/descargar `MazetotDemo.mp4`](MazetotDemo.mp4) (en la raíz del repo).

## Cómo correrlo

```bash
cargo run           # modo debug, corre a 60 fps sin problema
cargo run --release # si quieren más margen
```

Hay que correrlo desde la raíz del proyecto: ahí es donde vive `assets/` (texturas y audio) y
los `maze1.txt` / `maze2.txt` / `maze3.txt` que carga el menú de selección de nivel.

Si falta algún archivo de `assets/` (una textura o un sonido), el juego no truena: genera un
marcador de posición o simplemente reproduce en silencio esa pista.

## Controles

**Menú**

| Tecla | Acción |
|---|---|
| Flechas / `W` `S` | Moverse entre opciones |
| `ENTER` / `SPACE` | Elegir |
| `ESC` | Volver a la pantalla anterior del menú, o salir desde la principal |

**En el nivel**

| Tecla | Acción |
|---|---|
| `W` / `S` | Avanzar / retroceder |
| `A` / `D` | Moverse de lado (strafe) |
| Mouse | Girar la cámara |
| `SHIFT` | Correr (gasta energía) |
| `E` | Destruir el tótem cercano, o entrar/salir de un locker |
| `M` | Ver el mapa completo |
| `N` | Mostrar u ocultar el minimapa |
| `TAB` | Soltar o atrapar el mouse |
| `F1` | Guardar una captura en `maze.png` |
| `ESC` | Volver al menú (al ganar o perder) |

## Cómo se juega

Cada nivel tiene varios tótems repartidos por el laberinto. El primero que se rompe (`E` cerca
de uno) despierta al único enemigo del nivel; cada tótem siguiente lo acelera un poco más. La
puerta de salida (`g`) no se abre hasta que no queda ningún tótem en pie.

Mientras no lo ve, el enemigo patrulla solo. Si lo ve, lo persigue de verdad. Cada vez que se
rompe un tótem, el enemigo va directo a investigar dónde estaba el jugador en ese momento,
aunque no lo vea — como si hubiera oído el estruendo. Si llega ahí y no encuentra a nadie,
pierde el rastro y vuelve a patrullar. Esconderse en un locker cercano (`E`) es la forma de que
eso pase: mientras se está adentro, el enemigo no puede ver ni tocar al jugador.

## El archivo del laberinto

`maze1.txt`, `maze2.txt` y `maze3.txt` son texto plano donde cada caracter es una celda de
`BLOCK_SIZE` píxeles:

| Caracter | Significado |
|---|---|
| `+` `-` `\|` | Pared |
| espacio | Piso |
| `p` | Posición inicial del jugador |
| `g` | Meta (no abre hasta destruir todos los tótems) |
| `t` | Tótem a destruir |
| `e` | Punto donde aparece el enemigo (al romper el primer tótem) |
| `l` | Locker: cuenta como pared (bloquea el paso) y se puede usar desde la celda de al lado |

```
+--+--+--+--+
|p          |
+  +--+  +  +
|  t     |  |
+  +  +--+--+
|  |    l   |
+  +--+--+  +
|   e    | g|
+--+--+--+--+
```

Se puede editar o agregar un mapa nuevo: la ventana se dimensiona sola según el más grande de
los tres archivos (`ancho_en_caracteres * BLOCK_SIZE`). Las filas no necesitan medir todas lo
mismo — los espacios finales que borran los editores se tratan como piso.

## Estructura

```
src/
  main.rs         Ventana, loop de menú/nivel, raycasting 3D, piso/techo, HUD
  menu.rs         Menú principal, selector de nivel, pantalla de instrucciones
  maze.rs         Tipo Maze: carga el archivo, colisiones, línea de visión
  caster.rs       cast_ray: DDA, devuelve dónde y contra qué pegó cada rayo
  player.rs       Player (posición, ángulo, fov, vida, stamina) y su input
  enemy.rs        IA del enemigo: visión, estados (patrulla/investiga/persigue)
  pathfind.rs     Ruta más corta entre dos celdas (BFS sobre la grilla)
  totem.rs        Tótems destruibles y su zumbido de proximidad
  locker.rs       Locker empotrado en pared; esconderse de vista/daño
  sprites.rs      Billboards (enemigo, tótems, puerta) con oclusión por z-buffer
  textures.rs     TextureManager + generación procedural de texturas/sprites
  lighting.rs     Modelo de "linterna": caída por distancia + viñeteado
  audio.rs        Efectos y música; volumen dinámico según distancia
  framebuffer.rs  Buffer de pixeles propio, se sube a la ventana como textura
```

## Cómo funciona

**El rayo.** `cast_ray` usa DDA (Digital Differential Analysis): en vez de avanzar pixel por
pixel, salta de línea de cuadrícula en línea de cuadrícula hasta topar con una celda de pared,
lo que es más rápido y da la posición exacta del impacto (sin redondeo a 1px), necesaria para
texturizar sin artefactos. Devuelve un `Intersect` con la distancia, el caracter contra el que
pegó, `wall_x` (dónde a lo largo de esa cara, para elegir la columna de textura) y si fue una
pared horizontal o vertical (para sombrearlas distinto y remarcar las esquinas).

**El abanico.** Los rayos se reparten dentro del campo de visión (`fov = π/3`), empezando en
`a - fov/2` y avanzando una fracción del fov por rayo. El mapa 2D dibuja 120, el minimapa 40 y
la vista 3D lanza uno por cada columna de la pantalla.

**La proyección.** Cada distancia se vuelve una columna vertical de pared centrada en la mitad
de la pantalla:

```rust
let d = intersect.distance * (a - player.a).cos();
let stake_height = (BLOCK_SIZE as f32 / d) * distance_to_plane;
```

El `cos(a - player.a)` corrige el ojo de pez (los rayos de las orillas del fov recorren más
distancia que los del centro). `distance_to_plane = (ancho/2) / tan(fov/2)` en vez de una
constante calibrada a mano, así que cambiar el fov no obliga a reajustar la escala.

**Piso y techo.** Se pintan fila por fila (no columna por columna): a diferencia de las
paredes, toda una fila horizontal completa queda a la misma distancia real de la cámara sin
importar la columna (floor-casting estándar), así que se puede oscurecer con la misma linterna
sin repetir el cálculo por pixel.

**Los sprites.** Enemigo, tótems y puerta son billboards: siempre miran al jugador. Se dibujan
del más lejano al más cercano usando el mismo z-buffer que llenan las paredes, para que un
sprite detrás de una esquina quede recortado en vez de dibujarse encima de lo que debería
taparlo.

**La linterna.** Todo se ilumina con caída cuadrática por distancia más un viñeteado que oscurece
los bordes de la pantalla (el cono de una linterna real, no una luz pareja en todo el fov),
mezclando el color hacia un tono de sombra casi negro en vez de negro plano.

**La IA del enemigo.** Tres estados: `Idle` (patrulla), `Investigating` (va a la última celda
donde se supo del jugador) y `Chasing` (lo tiene a la vista). Tanto perseguir como investigar
calculan la ruta con un BFS sobre la grilla del laberinto (`pathfind.rs`) — con todas las celdas
al mismo costo, un BFS visita en el mismo orden que un Dijkstra pero sin necesitar la cola de
prioridad que ese algoritmo trae para pesos distintos — así el enemigo rodea las paredes en vez
de trabarse en las esquinas. La patrulla libre también se mueve celda a celda entre vecinas ya
confirmadas transitables, así que nunca choca contra un muro.

## Capturas

Estas son de una build bastante más temprana del proyecto (antes de texturas, iluminación,
tótems y enemigo), pero sirven para ver la técnica de raycasting en crudo: el mapa 2D con el
abanico de rayos y la proyección 3D resultante. Para ver cómo se ve el juego actual, el
[video](#video) de arriba.

![Vista 3D](capturas/vista3d.png)

![Mapa 2D](capturas/mapa2d.png)

## Tests

```bash
cargo test
```

No abre ventana. Cubre la trigonometría del raycaster (`cast_ray`, `wall_x`), colisiones y
línea de visión del laberinto, la IA del enemigo (visión, patrulla sin chocar, perseguir e
investigar con y sin obstáculos de por medio, perder el rastro), el pathfinding (`pathfind.rs`),
tótems (proximidad, destrucción), lockers y el manejo de vida/energía del jugador.
