use raylib::prelude::*;

/// Buffer de pixeles en RAM (RGBA8, 4 bytes por pixel) más la textura GPU a
/// la que se sube cada frame.
///
/// Antes cada `set_pixel` llamaba a `Image::draw_pixel`, una función de
/// raylib en C: con las paredes y sprites texturizados pixel por pixel eso
/// significa cientos de miles de llamadas FFI por frame, y ahí es donde se
/// iba el tiempo. Aquí `pixels` es un `Vec<u8>` que se escribe directo desde
/// Rust (sin cruzar a C por cada pixel), y la textura ya no se crea y
/// destruye cada frame: se crea una sola vez y se actualiza con
/// `update_texture`, que es la forma barata de subir los mismos bytes ya
/// reservados en la GPU en vez de pedir una textura nueva 60 veces por
/// segundo.
pub struct Framebuffer {
    pub width: i32,
    pub height: i32,
    pixels: Vec<u8>,
    texture: Texture2D,
    background_color: Color,
    current_color: Color,
}

impl Framebuffer {
    pub fn new(rl: &mut RaylibHandle, thread: &RaylibThread, width: i32, height: i32) -> Self {
        let background_color = Color::BLACK;
        let pixels = solid_pixels(width, height, background_color);

        let placeholder = Image::gen_image_color(width, height, background_color);
        let texture = rl
            .load_texture_from_image(thread, &placeholder)
            .expect("No se pudo crear la textura del framebuffer");

        Framebuffer {
            width,
            height,
            pixels,
            texture,
            background_color,
            current_color: Color::WHITE,
        }
    }

    pub fn set_background_color(&mut self, color: Color) {
        self.background_color = color;
    }

    pub fn set_current_color(&mut self, color: Color) {
        self.current_color = color;
    }

    pub fn clear(&mut self) {
        fill_pixels(&mut self.pixels, self.background_color);
    }

    pub fn set_pixel(&mut self, x: i32, y: i32) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let idx = self.index_of(x, y);
        write_color(&mut self.pixels[idx..idx + 4], self.current_color);
    }

    /// Rellena un rectángulo de un solo color escribiendo directo en el
    /// buffer, fila por fila.
    pub fn fill_rect(&mut self, x: i32, y: i32, width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + width).min(self.width);
        let y1 = (y + height).min(self.height);
        if x1 <= x0 || y1 <= y0 {
            return;
        }

        let color = self.current_color;
        let row_bytes = ((x1 - x0) * 4) as usize;
        for row in y0..y1 {
            let start = self.index_of(x0, row);
            fill_pixels(&mut self.pixels[start..start + row_bytes], color);
        }
    }

    /// Guarda el contenido actual del framebuffer como imagen. No es una
    /// operación en caliente (sólo se llama al presionar F1), así que aquí
    /// sí se justifica pasar por una `Image` de raylib para exportarla.
    pub fn render_to_file(&self, path: &str) {
        let mut image = Image::gen_image_color(self.width, self.height, Color::BLACK);
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = self.index_of(x, y);
                let c = Color::new(
                    self.pixels[idx],
                    self.pixels[idx + 1],
                    self.pixels[idx + 2],
                    self.pixels[idx + 3],
                );
                image.draw_pixel(x, y, c);
            }
        }
        image.export_image(path);
    }

    /// Sube el framebuffer a la textura GPU (ya existente, no una nueva) y
    /// la dibuja en la ventana.
    pub fn swap_buffers(&mut self, window: &mut RaylibHandle, thread: &RaylibThread) {
        let _ = self.texture.update_texture(&self.pixels);
        let mut renderer = window.begin_drawing(thread);
        renderer.draw_texture(&self.texture, 0, 0, Color::WHITE);
    }

    fn index_of(&self, x: i32, y: i32) -> usize {
        ((y * self.width + x) * 4) as usize
    }
}

fn solid_pixels(width: i32, height: i32, color: Color) -> Vec<u8> {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    fill_pixels(&mut pixels, color);
    pixels
}

fn fill_pixels(pixels: &mut [u8], color: Color) {
    for chunk in pixels.chunks_exact_mut(4) {
        write_color(chunk, color);
    }
}

fn write_color(dst: &mut [u8], color: Color) {
    dst[0] = color.r;
    dst[1] = color.g;
    dst[2] = color.b;
    dst[3] = color.a;
}
