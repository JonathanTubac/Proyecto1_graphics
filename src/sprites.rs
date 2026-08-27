use crate::framebuffer::Framebuffer;
use crate::maze::Maze;
use crate::player::Player;
use crate::textures::{TextureManager, TRANSPARENT_COLOR};
use raylib::prelude::*;
use std::f32::consts::PI;

/// Un objeto del mundo que siempre mira al jugador (billboard): enemigo,
/// item, etc. `texture` es el caracter con el que `TextureManager` busca su
/// imagen, igual que con las paredes.
pub struct Sprite {
    pub pos: Vector2,
    pub texture: char,
    /// Tamaño del sprite en el mundo, en las mismas unidades que `BLOCK_SIZE`
    /// (pixeles del framebuffer): un sprite del mismo tamaño que un bloque de
    /// pared se ve tan grande como esa pared a igual distancia.
    pub size: f32,
}

impl Sprite {
    pub fn new(pos: Vector2, texture: char, size: f32) -> Self {
        Sprite { pos, texture, size }
    }
}

/// Busca en el laberinto las celdas marcadas con `marker` y crea un sprite
/// centrado en cada una, del tamaño de un bloque.
pub fn spawn_from_maze(maze: &Maze, block_size: usize, marker: char, texture: char) -> Vec<Sprite> {
    let half = block_size as f32 / 2.0;
    maze.find_all(marker)
        .into_iter()
        .map(|(x, y)| {
            let pos = Vector2::new(
                x as f32 * block_size as f32 + half,
                y as f32 * block_size as f32 + half,
            );
            Sprite::new(pos, texture, block_size as f32)
        })
        .collect()
}

/// Compara sólo RGB: `Color` no deriva `PartialEq` en raylib-rs, y el canal
/// alfa siempre llega en 255 al decodificar un PNG, así que no aporta nada
/// a la comparación.
fn is_transparent(color: Color) -> bool {
    color.r == TRANSPARENT_COLOR.r && color.g == TRANSPARENT_COLOR.g && color.b == TRANSPARENT_COLOR.b
}

/// Dibuja todos los sprites visibles, del más lejano al más cercano. El
/// z-buffer ya trae, por columna, la distancia a la pared que se ve ahí; así
/// un sprite parcialmente detrás de una esquina se recorta correctamente en
/// vez de dibujarse encima de paredes que deberían taparlo.
///
/// Dibujar de atrás hacia adelante además resuelve el traslape entre dos
/// sprites: el más cercano queda pintado encima porque se dibuja al final.
pub fn draw_sprites(
    framebuffer: &mut Framebuffer,
    player: &Player,
    sprites: &[Sprite],
    textures: &TextureManager,
    z_buffer: &[f32],
) {
    let mut order: Vec<&Sprite> = sprites.iter().collect();
    order.sort_by(|a, b| {
        let da = (a.pos - player.pos).length_sqr();
        let db = (b.pos - player.pos).length_sqr();
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });

    for sprite in order {
        draw_sprite(framebuffer, player, sprite, textures, z_buffer);
    }
}

fn draw_sprite(
    framebuffer: &mut Framebuffer,
    player: &Player,
    sprite: &Sprite,
    textures: &TextureManager,
    z_buffer: &[f32],
) {
    let dx = sprite.pos.x - player.pos.x;
    let dy = sprite.pos.y - player.pos.y;

    // 1. Ángulo del jugador al sprite. atan2 (a diferencia de atan(dy/dx))
    //    conserva el cuadrante, así que funciona sin importar el signo de
    //    dx/dy y no se rompe cuando dx = 0.
    let angle_to_sprite = dy.atan2(dx);

    // 2. Diferencia angular normalizada a [-PI, PI], para no toparse con el
    //    salto de 2*PI a 0 al comparar ángulos.
    let mut angle_diff = angle_to_sprite - player.a;
    angle_diff = (angle_diff + PI).rem_euclid(2.0 * PI) - PI;

    let half_fov = player.fov / 2.0;
    // 3. Fuera del FOV: no se dibuja. Se da un pequeño margen extra sobre
    //    medio FOV para que el sprite no aparezca/desaparezca de golpe justo
    //    en el borde de la pantalla al girar la cámara.
    if angle_diff.abs() > half_fov + 0.15 {
        return;
    }

    // 4. Distancia euclidiana jugador-sprite. Se corrige por el mismo
    //    coseno que corrige el ojo de pez de las paredes: si no, el sprite
    //    quedaría mal escalado en los bordes de la pantalla y no calzaría
    //    contra el z-buffer (que sí está en distancia corregida).
    let raw_distance = (dx * dx + dy * dy).sqrt();
    let distance = (raw_distance * angle_diff.cos()).max(0.1);

    let width = framebuffer.width;
    let height = framebuffer.height;
    let half_height = height as f32 / 2.0;
    let distance_to_plane = (width as f32 / 2.0) / half_fov.tan();

    // 5. Tamaño en pantalla, inversamente proporcional a la distancia.
    let sprite_size = (sprite.size / distance) * distance_to_plane;
    if sprite_size < 1.0 {
        return; // demasiado lejos como para que se note un solo pixel
    }

    // Posición horizontal centrada: mismo criterio con el que `ray_angle`
    // reparte los rayos de las paredes en columnas, para que el sprite
    // quede alineado con la pared que tiene detrás en esa misma columna.
    let screen_x = ((angle_diff + half_fov) / player.fov) * width as f32;

    // Top-left del sprite en pantalla. Se guardan sin recortar (start_*_f)
    // porque tx/ty se calculan sobre el rectángulo completo, aunque la mitad
    // quede fuera de pantalla.
    let start_x_f = screen_x - sprite_size / 2.0;
    let start_y_f = half_height - sprite_size / 2.0;

    let start_x = start_x_f.max(0.0) as i32;
    let start_y = start_y_f.max(0.0) as i32;
    let end_x = (start_x_f + sprite_size).min(width as f32) as i32;
    let end_y = (start_y_f + sprite_size).min(height as f32) as i32;

    if end_x <= start_x || end_y <= start_y {
        return; // completamente fuera de pantalla
    }

    // Se resuelve la textura una sola vez para todo el sprite, en vez de
    // repetir el lookup en el HashMap por cada uno de sus pixeles.
    let Some(tex) = textures.resolve(sprite.texture) else {
        return;
    };

    for x in start_x..end_x {
        // 6. z-buffer: si la pared de esta columna está más cerca que el
        //    sprite, toda la columna queda tapada y ni se muestrea.
        if distance >= z_buffer[x as usize] {
            continue;
        }

        let u = (x as f32 - start_x_f) / sprite_size;

        for y in start_y..end_y {
            let v = (y as f32 - start_y_f) / sprite_size;

            // Mapeo de pixel de pantalla a pixel de textura.
            let color = tex.sample_uv(u, v);
            if is_transparent(color) {
                continue;
            }

            framebuffer.set_current_color(color);
            framebuffer.set_pixel(x, y);
        }
    }
}
