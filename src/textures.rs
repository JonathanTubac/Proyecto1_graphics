use raylib::prelude::*;
use std::collections::HashMap;
use std::path::Path;

/// Caracter de textura que se usa cuando un impacto no tiene una imagen
/// propia asignada (por ejemplo, un caracter de pared nuevo en el mapa que
/// todavía no tiene textura dedicada).
const FALLBACK_CHAR: char = '#';

/// Una textura ya decodificada en un buffer de colores en RAM. Se lee una
/// sola vez al cargar (`Image::get_image_data`) para poder muestrear pixel
/// por pixel cada frame con un simple acceso a `Vec`, sin volver a llamar a
/// raylib ni tocar punteros crudos por cada pixel dibujado.
struct TextureData {
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
        // Caracter de pared -> archivo de textura.
        let texture_files = [
            ('+', "assets/wall4.png"),
            ('-', "assets/wall2.png"),
            ('|', "assets/wall1.png"),
            ('g', "assets/wall5.png"),
            (FALLBACK_CHAR, "assets/wall3.png"), // default/fallback
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
    pub fn sample(&self, ch: char, u: f32, v: f32) -> Color {
        match self.texture_for(ch) {
            Some(tex) => {
                let tx = (u.clamp(0.0, 1.0) * (tex.width - 1) as f32).round() as u32;
                let ty = (v.clamp(0.0, 1.0) * (tex.height - 1) as f32).round() as u32;
                tex.sample(tx, ty)
            }
            None => Color::WHITE,
        }
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
        generate_placeholder_texture(*ch).export_image(path);
        println!("Textura '{ch}' no encontrada: generé un marcador de posición en {path}");
    }
}

/// Genera una textura de 64x64 con un patrón de ladrillo simple y un color
/// distinto por caracter, sólo para tener algo visualmente distinguible
/// mientras no se agregue arte definitivo en `assets/`.
fn generate_placeholder_texture(ch: char) -> Image {
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

    image
}

fn brick_palette(ch: char) -> (Color, Color) {
    match ch {
        '+' => (Color::new(150, 95, 70, 255), Color::new(60, 45, 40, 255)), // esquinas: ladrillo rojizo
        '-' => (Color::new(120, 120, 130, 255), Color::new(50, 50, 55, 255)), // horizontales: piedra gris
        '|' => (Color::new(95, 110, 150, 255), Color::new(40, 45, 60, 255)), // verticales: piedra azulada
        'g' => (Color::new(80, 190, 110, 255), Color::new(30, 90, 55, 255)), // meta: verde
        _ => (Color::new(160, 150, 100, 255), Color::new(70, 60, 40, 255)), // default: arena
    }
}
