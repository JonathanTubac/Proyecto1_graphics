use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use raylib::prelude::*;
use std::collections::HashMap;
use std::path::Path;

/// Caracter de textura que se usa cuando un impacto no tiene una imagen
/// propia asignada (por ejemplo, un caracter de pared nuevo en el mapa que
/// todavía no tiene textura dedicada).
const FALLBACK_CHAR: char = '#';

/// Caracteres que son sprites (billboards que miran al jugador) en vez de
/// texturas de pared: su marcador de posición se genera distinto (con fondo
/// transparente) y no participan del patrón de ladrillo.
const SPRITE_CHARS: &[char] = &['e', 'b', '1', '2', '3', '4', 'g', 't', 'l'];

/// Color reservado como "transparente" en las texturas de sprites: cualquier
/// pixel de ese color se salta al dibujarlo, dejando ver lo que haya detrás
/// en vez de un cuadro sólido. Es la técnica de color-key que usan los
/// raycasters clásicos que no manejan canal alpha real.
pub const TRANSPARENT_COLOR: Color = Color::new(152, 0, 136, 255);

/// Una textura ya decodificada en un buffer de colores en RAM. Se lee una
/// sola vez al cargar (`Image::get_image_data`) para poder muestrear pixel
/// por pixel cada frame con un simple acceso a `Vec`, sin volver a llamar a
/// raylib ni tocar punteros crudos por cada pixel dibujado.
pub struct TextureData {
    pixels: Vec<Color>,
    width: u32,
    height: u32,
}

impl TextureData {
    fn from_image(image: &Image) -> Self {
        TextureData {
            pixels: image.get_image_data().to_vec(),
            width: image.width().max(1) as u32,
            height: image.height().max(1) as u32,
        }
    }

    fn sample(&self, tx: u32, ty: u32) -> Color {
        let x = tx.min(self.width - 1);
        let y = ty.min(self.height - 1);
        self.pixels[(y * self.width + x) as usize]
    }

    /// Alto en pixeles de la textura fuente. Sirve para que quien dibuja un
    /// sprite muy agrandado (un enemigo cerca) sepa cuántas filas de
    /// pantalla comparten el mismo texel, y así no repetir el muestreo y el
    /// sombreado fila por fila cuando el resultado va a ser idéntico.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Muestrea con coordenadas normalizadas (0.0..1.0). Vive en `TextureData`
    /// (no sólo en `TextureManager::sample`) para que quien dibuja muchos
    /// pixeles seguidos de la misma textura (una rebanada de pared, un
    /// sprite) pueda resolverla una sola vez con `TextureManager::resolve` y
    /// evitar el lookup del `HashMap` en cada pixel.
    pub fn sample_uv(&self, u: f32, v: f32) -> Color {
        let tx = (u.clamp(0.0, 1.0) * (self.width - 1) as f32).round() as u32;
        let ty = (v.clamp(0.0, 1.0) * (self.height - 1) as f32).round() as u32;
        self.sample(tx, ty)
    }
}

pub struct TextureManager {
    data: HashMap<char, TextureData>, // Copia en RAM para leer pixeles individuales.
    // Copia en GPU: no la usa el raycaster por software (dibuja directo al
    // framebuffer en RAM), pero queda disponible como API pública, p.ej.
    // para un HUD o una vista previa de texturas.
    #[allow(dead_code)]
    textures: HashMap<char, Texture2D>,
}

impl TextureManager {
    pub fn new(rl: &mut RaylibHandle, thread: &RaylibThread) -> Self {
        // Caracter -> archivo de textura (paredes y sprites comparten el
        // mismo mapa; ver `SPRITE_CHARS` para saber cuáles son cuáles).
        let texture_files = [
            ('+', "assets/wall4.png"),
            ('-', "assets/wall2.png"),
            ('|', "assets/wall1.png"),
            (FALLBACK_CHAR, "assets/wall3.png"), // default/fallback
            ('e', "assets/sprite_enemy.png"),
            ('b', "assets/sprite_enemy_back.png"), // enemigo visto de espaldas
            ('1', "assets/sprite_enemy_run_a.png"), // corriendo, de frente, cuadro A
            ('2', "assets/sprite_enemy_run_b.png"), // corriendo, de frente, cuadro B
            ('3', "assets/sprite_enemy_run_back_a.png"), // corriendo, de espaldas, cuadro A
            ('4', "assets/sprite_enemy_run_back_b.png"), // corriendo, de espaldas, cuadro B
            ('g', "assets/sprite_door.png"), // meta: puerta de salida
            ('t', "assets/sprite_totem.png"), // tótem que hay que destruir
            ('l', "assets/sprite_locker.png"), // locker para esconderse del enemigo
        ];

        // El proyecto puede no traer arte todavía: en vez de que el programa
        // truene, generamos una textura de marcador de posición (patrón de
        // ladrillo con un color distinto por caracter) la primera vez que
        // falte un archivo, para que siempre haya algo que cargar.
        ensure_placeholder_assets(&texture_files);

        let mut data = HashMap::new();
        let mut textures = HashMap::new();

        for (ch, path) in texture_files {
            let image = match Image::load_image(path) {
                Ok(image) => image,
                Err(err) => {
                    eprintln!("No se pudo cargar la imagen '{path}': {err}. Se usará color plano.");
                    continue;
                }
            };

            data.insert(ch, TextureData::from_image(&image));

            match rl.load_texture(thread, path) {
                Ok(texture) => {
                    textures.insert(ch, texture);
                }
                Err(err) => eprintln!("No se pudo subir la textura '{path}' a la GPU: {err}"),
            }
        }

        TextureManager { data, textures }
    }

    /// Color del pixel (tx, ty), en coordenadas de pixel de la textura, para
    /// el caracter `ch`. Si `ch` no tiene textura propia cae a la textura por
    /// defecto; si tampoco existe, regresa blanco para no romper el render.
    #[allow(dead_code)]
    pub fn get_pixel_color(&self, ch: char, tx: u32, ty: u32) -> Color {
        self.texture_for(ch)
            .map(|tex| tex.sample(tx, ty))
            .unwrap_or(Color::WHITE)
    }

    /// Igual que `get_pixel_color`, pero recibe tx/ty normalizados en
    /// 0.0..1.0 (la fracción a lo largo de la pared y de la rebanada
    /// vertical), que es justo lo que produce el raycaster. Evita que cada
    /// llamador tenga que conocer el ancho/alto en pixeles de la textura.
    ///
    /// Para dibujar muchos pixeles seguidos de la misma textura (toda una
    /// rebanada de pared, todo un sprite) es mejor usar `resolve` una vez y
    /// llamar `sample_uv` sobre el resultado: así el lookup en el `HashMap`
    /// no se repite en cada pixel.
    #[allow(dead_code)]
    pub fn sample(&self, ch: char, u: f32, v: f32) -> Color {
        match self.texture_for(ch) {
            Some(tex) => tex.sample_uv(u, v),
            None => Color::WHITE,
        }
    }

    /// Resuelve una sola vez la textura de `ch` (con su fallback a `'#'`
    /// incluido), para muestrear muchos pixeles con `TextureData::sample_uv`
    /// sin repetir la búsqueda en el `HashMap` por cada uno.
    pub fn resolve(&self, ch: char) -> Option<&TextureData> {
        self.texture_for(ch)
    }

    #[allow(dead_code)]
    pub fn get_texture(&self, ch: char) -> Option<&Texture2D> {
        self.textures
            .get(&ch)
            .or_else(|| self.textures.get(&FALLBACK_CHAR))
    }

    fn texture_for(&self, ch: char) -> Option<&TextureData> {
        self.data.get(&ch).or_else(|| self.data.get(&FALLBACK_CHAR))
    }
}

fn ensure_placeholder_assets(files: &[(char, &str)]) {
    for (ch, path) in files {
        if Path::new(path).exists() {
            continue;
        }
        if let Some(dir) = Path::new(path).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let image = if SPRITE_CHARS.contains(ch) {
            generate_placeholder_sprite(*ch)
        } else {
            generate_placeholder_wall(*ch)
        };
        image.export_image(path);
        println!("Textura '{ch}' no encontrada: generé un marcador de posición en {path}");
    }
}

/// Genera un sprite sobre fondo transparente (color-key), con una forma
/// distinta según el caracter, sólo para tener algo visible mientras no se
/// agregue arte definitivo en `assets/`.
fn generate_placeholder_sprite(ch: char) -> Image {
    const SIZE: i32 = 64;
    let mut image = Image::gen_image_color(SIZE, SIZE, TRANSPARENT_COLOR);

    match ch {
        'g' => draw_door_shape(&mut image, SIZE),
        't' => draw_totem_shape(&mut image, SIZE),
        'l' => draw_locker_shape(&mut image, SIZE),
        _ => draw_humanoid_shape(&mut image, SIZE, ch),
    }

    image
}

/// Figura encapuchada sin rostro: nada más que una silueta casi negra y dos
/// ojos que "brillan" en el hueco de la capucha. Para un juego de terror
/// funciona mejor no dibujarle cara que intentar una cara amenazante: lo
/// que da miedo es no poder ver qué es, sólo que te está mirando.
///
/// `ch` decide tanto de qué lado se ve (frente, con ojos; espaldas, sin
/// ellos) como la pose de los brazos (quieto, o uno de los dos cuadros de
/// carrera que se alternan al ritmo de sus pasos — ver
/// `Enemy::sprite_for_viewer`).
fn draw_humanoid_shape(image: &mut Image, size: i32, ch: char) {
    let (body, edge, eye_glow, eye_core) = horror_palette(ch);
    let w = size as f32;
    let front = matches!(ch, 'e' | '1' | '2');
    let arms = arm_pose(ch);

    // Túnica: se ensancha de los "hombros" hacia el piso. Dos triángulos
    // superpuestos (el de abajo, un poco más chico) dan un borde con volumen
    // en vez de una silueta plana de un solo tono.
    image.draw_triangle(
        Vector2::new(w * 0.5, w * 0.22),
        Vector2::new(w * 0.06, w * 0.98),
        Vector2::new(w * 0.94, w * 0.98),
        edge,
    );
    image.draw_triangle(
        Vector2::new(w * 0.5, w * 0.28),
        Vector2::new(w * 0.14, w * 0.94),
        Vector2::new(w * 0.86, w * 0.94),
        body,
    );

    // Capucha: un círculo oscuro y hueco, sin rasgos. El vacío es lo que
    // inquieta, no una cara dibujada encima.
    image.draw_circle((w * 0.5) as i32, (w * 0.24) as i32, (w * 0.19) as i32, edge);
    image.draw_circle((w * 0.5) as i32, (w * 0.25) as i32, (w * 0.16) as i32, body);

    // Dobladillo irregular: se "muerde" la túnica con triángulos del propio
    // color transparente, como tela rasgada en vez de un corte parejo.
    for i in 0..4 {
        let x = w * (0.20 + i as f32 * 0.20);
        let depth = if i % 2 == 0 { 0.14 } else { 0.09 };
        image.draw_triangle(
            Vector2::new(x - w * 0.06, w * 0.94),
            Vector2::new(x + w * 0.06, w * 0.94),
            Vector2::new(x, w * (0.94 + depth)),
            TRANSPARENT_COLOR,
        );
    }

    // Brazos flacos colgando a los lados, más largos que el cuerpo. En las
    // poses de carrera se abren en direcciones opuestas para simular la
    // zancada; quieto, cuelgan derechos.
    image.draw_line_ex(
        Vector2::new(w * 0.12, w * 0.38),
        Vector2::new(w * (0.02 + arms.left_dx), w * (0.86 + arms.left_dy)),
        3,
        edge,
    );
    image.draw_line_ex(
        Vector2::new(w * 0.88, w * 0.38),
        Vector2::new(w * (0.98 + arms.right_dx), w * (0.86 + arms.right_dy)),
        3,
        edge,
    );

    if front {
        // Ojos: el único rasgo visible, mirando fijo desde el hueco de la
        // capucha. Con la linterna del jugador (ver `crate::lighting`) todo
        // lo que esté lejos se va casi a negro, así que estos siguen
        // viéndose brillar incluso cuando el resto de la figura ya se
        // perdió en la sombra.
        let eye_y = (w * 0.24) as i32;
        let eye_dx = (w * 0.075) as i32;
        for dx in [-eye_dx, eye_dx] {
            image.draw_circle((w * 0.5) as i32 + dx, eye_y, (w * 0.045) as i32, eye_glow);
            image.draw_circle((w * 0.5) as i32 + dx, eye_y, (w * 0.02) as i32, eye_core);
        }
    } else {
        // De espaldas no hay ojos que mostrar: en cambio, una costura al
        // centro de la capucha y la túnica, para que no se vea igual que un
        // frente sin ojos.
        image.draw_line(
            (w * 0.5) as i32,
            (w * 0.10) as i32,
            (w * 0.5) as i32,
            (w * 0.90) as i32,
            edge,
        );
    }
}

/// Corrimiento de cada brazo respecto a su posición en reposo, para las dos
/// poses de carrera. Se alternan entre sí para que, con la animación, se
/// vean como una zancada en vez de un solo cuadro estático.
struct ArmPose {
    left_dx: f32,
    left_dy: f32,
    right_dx: f32,
    right_dy: f32,
}

fn arm_pose(ch: char) -> ArmPose {
    match ch {
        '1' | '3' => ArmPose { left_dx: -0.07, left_dy: -0.10, right_dx: 0.07, right_dy: 0.10 },
        '2' | '4' => ArmPose { left_dx: 0.07, left_dy: 0.10, right_dx: -0.07, right_dy: -0.10 },
        _ => ArmPose { left_dx: 0.0, left_dy: 0.0, right_dx: 0.0, right_dy: 0.0 },
    }
}

fn horror_palette(ch: char) -> (Color, Color, Color, Color) {
    match ch {
        // enemigo (cualquier vista o pose): túnica casi negra, ojos rojo
        // brillante cuando se le ve de frente.
        'e' | 'b' | '1' | '2' | '3' | '4' => (
            Color::new(12, 10, 15, 255),
            Color::new(28, 22, 30, 255),
            Color::new(230, 15, 15, 255),
            Color::new(255, 210, 200, 255),
        ),
        // default: misma silueta, ojos ámbar para distinguirse del enemigo.
        _ => (
            Color::new(20, 18, 22, 255),
            Color::new(40, 34, 40, 255),
            Color::new(230, 200, 60, 255),
            Color::new(255, 240, 200, 255),
        ),
    }
}

/// Marco de madera con dos paneles hundidos y una perilla, para la puerta
/// de salida que se planta sobre la celda de meta ('g').
fn draw_door_shape(image: &mut Image, size: i32) {
    let frame = Color::new(90, 60, 30, 255);
    let panel = Color::new(150, 105, 55, 255);
    let inset = Color::new(170, 125, 70, 255);
    let knob = Color::new(230, 200, 80, 255);

    image.draw_rectangle(4, 2, size - 8, size - 4, frame);
    image.draw_rectangle(8, 6, size - 16, size - 12, panel);
    image.draw_rectangle(14, 10, size - 28, size / 2 - 14, inset);
    image.draw_rectangle(14, size / 2 + 2, size - 28, size / 2 - 14, inset);
    image.draw_circle(size - 18, size * 3 / 5, 3, knob);
}

/// Ídolo de piedra tallado en bloques apilados (cada uno más angosto hacia
/// arriba), con grietas y una runa que brilla en morado en el bloque de en
/// medio: lo único con color en toda la figura, para que se note que tiene
/// algo maligno adentro. Es lo que el jugador tiene que destruir.
fn draw_totem_shape(image: &mut Image, size: i32) {
    let stone = Color::new(58, 52, 50, 255);
    let stone_dark = Color::new(28, 25, 24, 255);
    let rune_glow = Color::new(150, 20, 190, 255);
    let rune_core = Color::new(230, 190, 255, 255);

    let w = size as f32;

    // Base ancha, cabeza angosta: cuatro bloques apilados, como un tótem
    // tallado en piezas.
    let blocks = [
        (0.08, 0.72, 0.92, 0.98),
        (0.16, 0.46, 0.84, 0.74),
        (0.24, 0.20, 0.76, 0.48),
        (0.32, 0.02, 0.68, 0.22),
    ];
    for (x0, y0, x1, y1) in blocks {
        image.draw_rectangle(
            (w * x0) as i32,
            (w * y0) as i32,
            (w * (x1 - x0)) as i32,
            (w * (y1 - y0)) as i32,
            stone,
        );
    }

    // Línea divisoria entre cada bloque, para que se note tallado en piezas
    // y no una sola columna lisa.
    for &(_, y0, _, _) in blocks.iter().skip(1) {
        image.draw_line(6, (w * y0) as i32, size - 6, (w * y0) as i32, stone_dark);
    }

    // Un par de grietas.
    image.draw_line((w * 0.30) as i32, (w * 0.50) as i32, (w * 0.36) as i32, (w * 0.70) as i32, stone_dark);
    image.draw_line((w * 0.66) as i32, (w * 0.28) as i32, (w * 0.60) as i32, (w * 0.44) as i32, stone_dark);

    // La runa: el único rasgo con color en toda la figura.
    let cx = (w * 0.5) as i32;
    let cy = (w * 0.34) as i32;
    image.draw_circle(cx, cy, (w * 0.07) as i32, rune_glow);
    image.draw_circle(cx, cy, (w * 0.03) as i32, rune_core);
}

/// Casillero metálico donde el jugador se esconde del enemigo (ver
/// `crate::locker`): puerta con rejillas de ventilación, línea central como
/// si fueran dos hojas, manija con candado y un par de rayones/óxido para
/// que no se vea impecable. A diferencia de los sprites humanoides, cubre
/// todo el cuadro (nada de fondo transparente): es un mueble sólido, no
/// algo que se recorte contra el fondo.
fn draw_locker_shape(image: &mut Image, size: i32) {
    // Más claro que un casillero "real" a propósito: con la linterna tan
    // tenue (ver `crate::lighting`), un metal tan oscuro como el de una
    // pared se pierde contra ella a cualquier distancia razonable. Este es
    // el único mueble con el que el jugador necesita toparse a tiempo
    // mientras huye, así que tiene que notarse antes de estar encima.
    let metal = Color::new(92, 100, 110, 255);
    let metal_dark = Color::new(48, 52, 58, 255);
    let vent = Color::new(28, 30, 34, 255);
    let handle = Color::new(205, 180, 60, 255);
    // Lucecita de "salida de emergencia": el mismo lenguaje visual que la
    // runa brillante del tótem (ver `draw_totem_shape`) para que, aun casi a
    // oscuras, el jugador reconozca de un vistazo "esto es un refugio", no
    // sólo una mancha más clara en la pared.
    let led_glow = Color::new(70, 230, 120, 255);
    let led_core = Color::new(210, 255, 220, 255);
    let w = size as f32;

    image.draw_rectangle(0, 0, size, size, metal_dark);
    image.draw_rectangle(
        (w * 0.06) as i32,
        (w * 0.04) as i32,
        (w * 0.88) as i32,
        (w * 0.92) as i32,
        metal,
    );

    // Rejillas de ventilación cerca de arriba, como en un casillero real.
    for i in 0..5 {
        let y = (w * (0.14 + i as f32 * 0.035)) as i32;
        image.draw_rectangle((w * 0.14) as i32, y, (w * 0.72) as i32, (w * 0.02) as i32, vent);
    }

    // Línea central: como si la puerta fueran dos hojas.
    image.draw_line((w * 0.5) as i32, (w * 0.04) as i32, (w * 0.5) as i32, (w * 0.96) as i32, metal_dark);

    // Manija con candado, a un lado de la línea central.
    image.draw_rectangle((w * 0.56) as i32, (w * 0.48) as i32, (w * 0.08) as i32, (w * 0.14) as i32, handle);
    image.draw_circle((w * 0.6) as i32, (w * 0.5) as i32, (w * 0.02) as i32, metal_dark);

    // Rayones/óxido, para que no se vea impecable.
    image.draw_line((w * 0.2) as i32, (w * 0.6) as i32, (w * 0.3) as i32, (w * 0.85) as i32, metal_dark);
    image.draw_line((w * 0.7) as i32, (w * 0.3) as i32, (w * 0.78) as i32, (w * 0.5) as i32, metal_dark);

    // Lucecita sobre cada hoja: el único color saturado de todo el sprite,
    // igual que la runa del tótem, para que siga leyéndose como "aquí hay
    // algo" aunque la linterna ya casi no llegue.
    for cx in [w * 0.25, w * 0.75] {
        let cy = (w * 0.10) as i32;
        image.draw_circle(cx as i32, cy, (w * 0.035) as i32, led_glow);
        image.draw_circle(cx as i32, cy, (w * 0.015) as i32, led_core);
    }
}

/// Genera una textura de 64x64 con un patrón de ladrillo simple, un color
/// oscuro y desaturado distinto por caracter, y encima mugre/grietas/
/// churretes para que se sienta una pared vieja y húmeda en vez de un
/// ladrillo limpio de catálogo. Sólo para tener algo visualmente
/// distinguible mientras no se agregue arte definitivo en `assets/`.
fn generate_placeholder_wall(ch: char) -> Image {
    const SIZE: i32 = 64;
    const ROWS: i32 = 8;
    const ROW_H: i32 = SIZE / ROWS;

    let (brick, mortar) = brick_palette(ch);
    let mut image = Image::gen_image_color(SIZE, SIZE, mortar);

    for row in 0..ROWS {
        let y = row * ROW_H;
        // Hiladas alternadas (offset a medio ladrillo) como en un muro real.
        let offset = if row % 2 == 0 { 0 } else { ROW_H };
        let mut x = -offset;
        while x < SIZE {
            image.draw_rectangle(x + 1, y + 1, ROW_H * 2 - 2, ROW_H - 2, brick);
            x += ROW_H * 2;
        }
    }

    grime_and_decay(&mut image, ch, SIZE);

    image
}

/// Ensucia una textura de pared ya generada: manchas de humedad/musgo,
/// churretes verticales (agua u óxido escurriendo) y grietas quebradas. La
/// semilla depende del caracter, así que cada tipo de pared tiene siempre
/// el mismo patrón de mugre entre corridas, en vez de cambiar cada vez que
/// se regenera el marcador de posición.
fn grime_and_decay(image: &mut Image, ch: char, size: i32) {
    let mut rng = StdRng::seed_from_u64(ch as u64);

    let grime = Color::new(14, 17, 13, 255);
    let stain = Color::new(18, 12, 10, 255);
    let crack = Color::new(8, 7, 7, 255);

    // Manchas de musgo/humedad: racimos de círculos chicos e irregulares.
    for _ in 0..7 {
        let cx = rng.gen_range(0..size);
        let cy = rng.gen_range(0..size);
        let blobs = rng.gen_range(3..6);
        for _ in 0..blobs {
            let ox = (cx + rng.gen_range(-6..6)).clamp(0, size - 1);
            let oy = (cy + rng.gen_range(-6..6)).clamp(0, size - 1);
            let r = rng.gen_range(2..5);
            image.draw_circle(ox, oy, r, grime);
        }
    }

    // Churretes verticales, de arriba hacia abajo, como si algo hubiera
    // escurrido por la pared.
    for _ in 0..3 {
        let x = rng.gen_range(4..size - 4);
        let start_y = rng.gen_range(0..size / 3);
        let end_y = rng.gen_range(size / 2..size);
        let drift = rng.gen_range(-2..2);
        image.draw_line(x, start_y, x + drift, end_y, stain);
    }

    // Grietas: líneas quebradas cortas, en 3-4 segmentos.
    for _ in 0..4 {
        let mut x = rng.gen_range(0..size);
        let mut y = rng.gen_range(0..size);
        for _ in 0..3 {
            let nx = (x + rng.gen_range(-8..8)).clamp(0, size - 1);
            let ny = (y + rng.gen_range(-8..8)).clamp(0, size - 1);
            image.draw_line(x, y, nx, ny, crack);
            x = nx;
            y = ny;
        }
    }
}

fn brick_palette(ch: char) -> (Color, Color) {
    match ch {
        '+' => (Color::new(75, 55, 48, 255), Color::new(30, 24, 22, 255)), // esquinas: ladrillo húmedo
        '-' => (Color::new(55, 58, 56, 255), Color::new(22, 24, 23, 255)), // horizontales: piedra sucia
        '|' => (Color::new(48, 58, 60, 255), Color::new(18, 24, 25, 255)), // verticales: piedra fría
        _ => (Color::new(70, 62, 48, 255), Color::new(28, 24, 18, 255)),  // default: tierra/arena vieja
    }
}
