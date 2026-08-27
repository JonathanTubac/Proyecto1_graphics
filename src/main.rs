mod caster;
mod enemy;
mod framebuffer;
mod lighting;
mod maze;
mod player;
mod sprites;
mod textures;
mod totem;

use caster::{cast_ray, draw_ray_path};
use framebuffer::Framebuffer;
use maze::{Maze, load_maze};
use player::{Player, process_events};
use raylib::prelude::*;
use sprites::{Sprite, draw_sprites};
use std::f32::consts::PI;
use textures::TextureManager;

const BLOCK_SIZE: usize = 40;
/// Rayos del abanico en la vista 2D. En 3D se lanza uno por columna.
const NUM_RAYS_2D: usize = 120;
/// Rayos del abanico dibujado en el minimapa (menos, es más chico).
const NUM_RAYS_MINIMAP: usize = 40;
/// Qué tanto se encoge el mapa al dibujarlo como minimapa.
const MINIMAP_SCALE: f32 = 0.22;
/// Separación del minimapa respecto a la esquina de la pantalla.
const MINIMAP_MARGIN: i32 = 12;

/// Colores del mapa 2D y el minimapa. Apagados a propósito: aunque el mapa
/// es una vista de utilidad (no rompe la inmersión con colores alegres),
/// sigue siendo legible.
const MAP_WALL_COLOR: Color = Color::new(70, 65, 90, 255);
const MAP_GOAL_COLOR: Color = Color::new(45, 110, 75, 255);
/// Cielo/vacío por encima de las paredes en la vista 3D: casi negro, apenas
/// con un dejo de color para no ser un negro plano.
const SKY_COLOR: Color = Color::new(9, 8, 13, 255);
/// Piso: tierra oscura y húmeda, no un marrón cálido de catálogo.
const FLOOR_COLOR: Color = Color::new(24, 21, 19, 255);
/// Colores de los puntitos en el mapa 2D/minimapa, para distinguir de un
/// vistazo amenazas (enemigos) de objetivos (tótems).
const ENEMY_MARKER_COLOR: Color = Color::new(220, 60, 60, 255);
const TOTEM_MARKER_COLOR: Color = Color::new(170, 70, 210, 255);

/// Vista activa: el mapa desde arriba o la proyección desde los ojos del jugador.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Map2D,
    World3D,
}

/// Si la partida sigue en curso, ya se llegó a la puerta de salida, o se
/// perdieron los tres corazones.
#[derive(Clone, Copy, PartialEq)]
enum GameState {
    Playing,
    Won,
    Lost,
}

/// Ángulo del rayo `i` de `total`, repartidos dentro del fov del jugador.
/// El primero sale en a - fov/2 y de ahí avanza una fracción del fov.
fn ray_angle(player: &Player, i: usize, total: usize) -> f32 {
    let current_ray = i as f32 / total as f32;
    player.a - (player.fov / 2.0) + (player.fov * current_ray)
}

/// Pinta un bloque sólido de `size` x `size` con la esquina en (x0, y0).
fn draw_block(framebuffer: &mut Framebuffer, x0: i32, y0: i32, size: i32, color: Color) {
    framebuffer.set_current_color(color);
    framebuffer.fill_rect(x0, y0, size, size);
}

/// Dibuja el laberinto completo: cada caracter del archivo es una celda.
fn render_maze(framebuffer: &mut Framebuffer, maze: &Maze) {
    let block = BLOCK_SIZE as i32;

    for y in 0..maze.height() {
        for x in 0..maze.width() {
            let x0 = x as i32 * block;
            let y0 = y as i32 * block;

            if maze.is_wall(x, y) {
                draw_block(framebuffer, x0, y0, block, MAP_WALL_COLOR);
            } else if maze.get(x, y) == 'g' {
                draw_block(framebuffer, x0, y0, block, MAP_GOAL_COLOR);
            }
        }
    }
}

/// Dibuja al jugador como un cuadrito centrado en su posición.
fn render_player(framebuffer: &mut Framebuffer, player: &Player) {
    let size = (BLOCK_SIZE as i32 / 2).max(2);
    draw_block(
        framebuffer,
        player.pos.x as i32 - size / 2,
        player.pos.y as i32 - size / 2,
        size,
        Color::new(255, 220, 0, 255),
    );
}

/// Marca la posición de cada sprite con un puntito de `color`, para
/// ubicarlos de un vistazo en el mapa 2D (no tiene nada que ver con el
/// billboard en 3D). El color lo elige quien llama para poder distinguir,
/// p.ej., enemigos de tótems.
fn render_sprite_markers(framebuffer: &mut Framebuffer, sprites: &[Sprite], color: Color) {
    let size = (BLOCK_SIZE as i32 / 3).max(2);
    for sprite in sprites {
        draw_block(
            framebuffer,
            sprite.pos.x as i32 - size / 2,
            sprite.pos.y as i32 - size / 2,
            size,
            color,
        );
    }
}

/// Vista de arriba: el laberinto, el abanico de rayos y el jugador.
fn render_map2d(
    framebuffer: &mut Framebuffer,
    maze: &Maze,
    player: &Player,
    enemies: &[Sprite],
    totems: &[Sprite],
) {
    framebuffer.clear();
    render_maze(framebuffer, maze);

    for i in 0..NUM_RAYS_2D {
        let a = ray_angle(player, i, NUM_RAYS_2D);
        let intersect = cast_ray(maze, player, a, BLOCK_SIZE);
        draw_ray_path(framebuffer, player, a, intersect.distance);
    }

    render_sprite_markers(framebuffer, enemies, ENEMY_MARKER_COLOR);
    render_sprite_markers(framebuffer, totems, TOTEM_MARKER_COLOR);
    // El jugador va al final para que el cuadrito quede encima de todo.
    render_player(framebuffer, player);
}

/// Vista en primera persona: un rayo por columna de pantalla y cada distancia
/// se convierte en la altura de esa columna de pared, texturizada pixel por
/// pixel a partir del punto exacto donde el rayo pegó. Al final se dibujan
/// los sprites encima, usando el z-buffer que se llena de paso para que
/// queden tapados por las paredes que estén más cerca.
fn render_world3d(
    framebuffer: &mut Framebuffer,
    maze: &Maze,
    player: &Player,
    textures: &TextureManager,
    enemies: &[Sprite],
) {
    let width = framebuffer.width;
    let height = framebuffer.height;
    let half_height = height as f32 / 2.0;
    let num_rays = width as usize;

    let half_fov = player.fov / 2.0;
    // Distancia al plano de proyección: con esto una pared a BLOCK_SIZE de
    // distancia ocupa justo el alto de la pantalla.
    let distance_to_plane = (width as f32 / 2.0) / half_fov.tan();

    // Distancia (ya corregida de ojo de pez) de la pared más cercana en cada
    // columna. Los sprites la usan para saber si quedan tapados.
    let mut z_buffer = vec![f32::INFINITY; num_rays];

    for i in 0..num_rays {
        let a = ray_angle(player, i, num_rays);
        let intersect = cast_ray(maze, player, a, BLOCK_SIZE);

        // Corrección de ojo de pez: la distancia útil es la proyectada sobre
        // la dirección de vista, no la del rayo.
        let d = (intersect.distance * (a - player.a).cos()).max(0.1);
        z_buffer[i] = d;
        let stake_height = (BLOCK_SIZE as f32 / d) * distance_to_plane;

        // Extremos sin recortar: se necesitan completos para calcular ty,
        // aunque la mitad de la rebanada quede fuera de pantalla.
        let top_f = half_height - stake_height / 2.0;
        let bottom_f = half_height + stake_height / 2.0;
        let x = i as i32;
        let top = (top_f as i32).clamp(0, height);
        let bottom = (bottom_f as i32).clamp(0, height);

        framebuffer.set_current_color(SKY_COLOR);
        framebuffer.fill_rect(x, 0, 1, top);
        framebuffer.set_current_color(FLOOR_COLOR);
        framebuffer.fill_rect(x, bottom, 1, height - bottom);

        if intersect.impact == ' ' {
            continue; // El rayo se salió del mapa: no hay pared que texturizar.
        }

        // Se resuelve la textura de esta columna una sola vez: todos sus
        // pixeles son del mismo caracter de pared, así que repetir el
        // lookup en el HashMap por cada uno sería puro desperdicio.
        let Some(tex) = textures.resolve(intersect.impact) else {
            continue;
        };

        // La "linterna" del jugador: cerca y al centro de la vista, bien
        // iluminado; lejos o hacia los bordes, se va a negro. Entre dos
        // paredes a la misma distancia, la que se vio de canto (north/south)
        // se oscurece un poco más que la de frente (east/west), para que se
        // distingan las esquinas del laberinto aunque compartan textura.
        let mut shade = lighting::torch_intensity(d, a - player.a, half_fov);
        if intersect.side {
            shade *= 0.75;
        }

        for y in top..bottom {
            let ty = ((y as f32 - top_f) / stake_height).clamp(0.0, 1.0);
            let texel = tex.sample_uv(intersect.wall_x, ty);
            framebuffer.set_current_color(lighting::apply(texel, shade));
            framebuffer.set_pixel(x, y);
        }
    }

    draw_sprites(framebuffer, player, enemies, textures, &z_buffer);
}

/// El mismo mapa 2D pero encogido y en la esquina, encima de la vista 3D.
fn render_minimap(
    framebuffer: &mut Framebuffer,
    maze: &Maze,
    player: &Player,
    enemies: &[Sprite],
    totems: &[Sprite],
) {
    let block = ((BLOCK_SIZE as f32) * MINIMAP_SCALE).round().max(2.0) as i32;
    // Factor para pasar de pixeles del mundo a pixeles del minimapa.
    let scale = block as f32 / BLOCK_SIZE as f32;
    let ox = MINIMAP_MARGIN;
    let oy = MINIMAP_MARGIN;

    // Fondo opaco: si no, la vista 3D se ve por debajo de los pasillos.
    let w = maze.width() as i32 * block;
    let h = maze.height() as i32 * block;
    framebuffer.set_current_color(Color::new(15, 15, 22, 255));
    framebuffer.fill_rect(ox - 3, oy - 3, w + 6, h + 6);

    for y in 0..maze.height() {
        for x in 0..maze.width() {
            let color = if maze.is_wall(x, y) {
                MAP_WALL_COLOR
            } else if maze.get(x, y) == 'g' {
                MAP_GOAL_COLOR
            } else {
                continue;
            };
            framebuffer.set_current_color(color);
            framebuffer.fill_rect(ox + x as i32 * block, oy + y as i32 * block, block, block);
        }
    }

    // Los rayos se lanzan en coordenadas del mundo y se dibujan escalados,
    // por eso aquí cast_ray va con draw_line en false.
    for i in 0..NUM_RAYS_MINIMAP {
        let a = ray_angle(player, i, NUM_RAYS_MINIMAP);
        let hit = cast_ray(maze, player, a, BLOCK_SIZE);

        framebuffer.set_current_color(Color::new(230, 230, 240, 255));
        let (cos, sin) = (a.cos(), a.sin());
        let mut d = 0.0;
        while d < hit.distance {
            let px = ox + ((player.pos.x + d * cos) * scale) as i32;
            let py = oy + ((player.pos.y + d * sin) * scale) as i32;
            framebuffer.set_pixel(px, py);
            d += 1.0;
        }
    }

    let dot = (block / 2).max(3);

    framebuffer.set_current_color(ENEMY_MARKER_COLOR);
    for sprite in enemies {
        framebuffer.fill_rect(
            ox + (sprite.pos.x * scale) as i32 - dot / 2,
            oy + (sprite.pos.y * scale) as i32 - dot / 2,
            dot,
            dot,
        );
    }

    framebuffer.set_current_color(TOTEM_MARKER_COLOR);
    for sprite in totems {
        framebuffer.fill_rect(
            ox + (sprite.pos.x * scale) as i32 - dot / 2,
            oy + (sprite.pos.y * scale) as i32 - dot / 2,
            dot,
            dot,
        );
    }

    framebuffer.set_current_color(Color::new(255, 220, 0, 255));
    framebuffer.fill_rect(
        ox + (player.pos.x * scale) as i32 - dot / 2,
        oy + (player.pos.y * scale) as i32 - dot / 2,
        dot,
        dot,
    );
}

/// Pantalla de fin de partida (victoria o derrota): un velo oscuro
/// semitransparente sobre la última escena renderizada (que queda
/// congelada) más un mensaje centrado. Se dibuja directo con las
/// primitivas de raylib (no con el framebuffer de software) porque necesita
/// texto, y nuestro framebuffer no sabe dibujar texto. `draw_text` no
/// soporta UTF-8, así que los mensajes van sin acentos.
fn draw_end_screen(
    d: &mut RaylibDrawHandle,
    width: i32,
    height: i32,
    title: &str,
    title_color: Color,
    hint: &str,
) {
    d.draw_rectangle(0, 0, width, height, Color::new(0, 0, 0, 180));

    let title_size = 64;
    let title_width = d.measure_text(title, title_size);
    d.draw_text(
        title,
        width / 2 - title_width / 2,
        height / 2 - title_size,
        title_size,
        title_color,
    );

    let hint_size = 22;
    let hint_width = d.measure_text(hint, hint_size);
    d.draw_text(
        hint,
        width / 2 - hint_width / 2,
        height / 2 + 16,
        hint_size,
        Color::new(230, 230, 230, 255),
    );
}

/// Un corazón simple (dos círculos + un triángulo) relleno si todavía cuenta
/// como vida, o apenas un contorno tenue si ya se perdió.
fn draw_heart(d: &mut RaylibDrawHandle, cx: i32, cy: i32, size: i32, filled: bool) {
    let color = if filled {
        Color::new(220, 40, 60, 255)
    } else {
        Color::new(70, 70, 78, 255)
    };
    let lobe_r = (size / 4) as f32;
    let lobe_y = cy - size / 6;

    d.draw_circle(cx - size / 4, lobe_y, lobe_r, color);
    d.draw_circle(cx + size / 4, lobe_y, lobe_r, color);
    d.draw_triangle(
        Vector2::new((cx - size / 2) as f32, lobe_y as f32),
        Vector2::new((cx + size / 2) as f32, lobe_y as f32),
        Vector2::new(cx as f32, (cy + size / 2) as f32),
        color,
    );
}

/// HUD de vidas: `max_lives` corazones arriba a la derecha, los primeros
/// `lives` rellenos y el resto apagados.
fn draw_hearts_hud(d: &mut RaylibDrawHandle, width: i32, lives: u32, max_lives: u32) {
    const SIZE: i32 = 26;
    const SPACING: i32 = 8;
    const MARGIN: i32 = 16;

    let total_width = max_lives as i32 * SIZE + (max_lives as i32 - 1) * SPACING;
    let start_x = width - MARGIN - total_width + SIZE / 2;
    let cy = MARGIN + SIZE / 2;

    for i in 0..max_lives {
        let cx = start_x + i as i32 * (SIZE + SPACING);
        draw_heart(d, cx, cy, SIZE, i < lives);
    }
}

/// Contador de tótems destruidos, justo debajo de los corazones.
fn draw_totem_counter(d: &mut RaylibDrawHandle, width: i32, destroyed: usize, total: usize) {
    let text = format!("Totems: {destroyed}/{total}");
    let size = 20;
    let text_width = d.measure_text(&text, size);
    d.draw_text(&text, width - 16 - text_width, 16 + 26 + 8, size, Color::new(200, 160, 230, 255));
}

/// Pista de "puedes interactuar" cuando el jugador está junto a un tótem
/// sin destruir todavía.
fn draw_interact_hint(d: &mut RaylibDrawHandle, width: i32, height: i32) {
    let text = "[E] Destruir totem";
    let size = 22;
    let text_width = d.measure_text(text, size);
    d.draw_text(text, width / 2 - text_width / 2, height - 60, size, Color::new(230, 230, 230, 255));
}

/// Pista al pisar la meta sin haber destruido todos los tótems: la puerta
/// no se abre todavía.
fn draw_locked_door_hint(d: &mut RaylibDrawHandle, width: i32, remaining: usize) {
    let text = format!("La puerta esta sellada. Faltan {remaining} totem(s).");
    let size = 22;
    let text_width = d.measure_text(&text, size);
    d.draw_text(&text, width / 2 - text_width / 2, 60, size, Color::new(230, 120, 120, 255));
}

/// `enemy_markers`/`totem_markers` son los sprites que se marcan como
/// puntitos en el mapa 2D (para ubicar amenazas y objetivos de un vistazo);
/// `world` es todo lo que se dibuja como billboard en la vista 3D (enemigos
/// + puerta de meta + tótems sin destruir). La puerta no lleva marcador
/// propio: ya se ve como celda verde en el mapa.
fn render(
    framebuffer: &mut Framebuffer,
    maze: &Maze,
    player: &Player,
    mode: Mode,
    show_minimap: bool,
    textures: &TextureManager,
    enemy_markers: &[Sprite],
    totem_markers: &[Sprite],
    world: &[Sprite],
) {
    match mode {
        Mode::Map2D => render_map2d(framebuffer, maze, player, enemy_markers, totem_markers),
        Mode::World3D => {
            render_world3d(framebuffer, maze, player, textures, world);
            if show_minimap {
                render_minimap(framebuffer, maze, player, enemy_markers, totem_markers);
            }
        }
    }
}

fn main() {
    let maze = load_maze("maze.txt");

    let width = (maze.width() * BLOCK_SIZE) as i32;
    let height = (maze.height() * BLOCK_SIZE) as i32;

    let (start_x, start_y) = maze
        .player_start()
        .expect("El laberinto no tiene una 'p' para el jugador");
    let mut player = Player::at_cell(start_x, start_y, BLOCK_SIZE, PI / 3.0, PI / 3.0);

    let (mut window, thread) = raylib::init()
        .size(width, height)
        .title("Laberinto")
        .build();
    window.set_target_fps(60);
    // Oculta y centra el cursor en cada frame: así get_mouse_delta() da
    // movimiento relativo continuo en vez de toparse con el borde de la
    // ventana, que es lo que se necesita para girar la cámara con el mouse.
    window.disable_cursor();

    let mut framebuffer = Framebuffer::new(&mut window, &thread, width, height);
    framebuffer.set_background_color(Color::new(10, 9, 13, 255));
    let textures = TextureManager::new(&mut window, &thread);
    let mut enemies = enemy::spawn_from_maze(&maze, BLOCK_SIZE, 'e', 'e', 0.0);
    // La puerta de salida es un sprite fijo, no un Enemy: no persigue ni ve,
    // sólo se planta sobre la celda de meta para poder verla en 3D.
    let door_sprites = sprites::spawn_from_maze(&maze, BLOCK_SIZE, 'g', 'g');
    let mut totems = totem::spawn_from_maze(&maze, BLOCK_SIZE, 't');
    let mut mode = Mode::World3D;
    let mut show_minimap = true;
    let mut game_state = GameState::Playing;

    println!(
        "Jugador en celda {:?}, meta en {:?}, {} enemigos, {} totems, fov {:.2} rad",
        (start_x, start_y),
        maze.goal(),
        enemies.len(),
        totems.len(),
        player.fov
    );
    println!(
        "W/S: avanzar | A/D: strafe | mouse: girar | E: destruir totem cercano | M: mapa completo | N: minimapa | TAB: soltar el mouse | F1: guardar maze.png"
    );

    while !window.window_should_close() {
        // Sólo importan mientras se sigue jugando; se usan más abajo para
        // decidir qué pistas dibujar en el HUD.
        let mut near_totem = false;
        let mut on_locked_goal = false;

        if game_state == GameState::Playing {
            process_events(&mut player, &window, &maze, BLOCK_SIZE);
            enemy::update_enemies(&mut enemies, &player, &maze, BLOCK_SIZE);

            if let Some(idx) = totem::interactable(&totems, &player) {
                near_totem = true;
                if window.is_key_pressed(KeyboardKey::KEY_E) {
                    totem::destroy(&mut totems, idx);
                }
            }

            // Se llegó a la puerta: basta con pisar la celda de meta, sin
            // necesitar un radio de colisión aparte (es la misma celda 'g'
            // sobre la que se para el sprite de la puerta). Pero sólo abre
            // si ya no queda ningún tótem en pie.
            let cell_x = (player.pos.x / BLOCK_SIZE as f32) as usize;
            let cell_y = (player.pos.y / BLOCK_SIZE as f32) as usize;
            if maze.get(cell_x, cell_y) == 'g' {
                if totem::all_destroyed(&totems) {
                    game_state = GameState::Won;
                    window.enable_cursor();
                } else {
                    on_locked_goal = true;
                }
            }

            // Sólo tiene sentido revisar daño si no se ganó ya en este mismo
            // frame (llegar a la meta con un enemigo pegado cuenta como
            // ganar, no como perder).
            if game_state == GameState::Playing {
                player.tick();
                enemy::damage_player_if_close(&enemies, &mut player);
                if player.lives == 0 {
                    game_state = GameState::Lost;
                    window.enable_cursor();
                }
            }
        }

        if window.is_key_pressed(KeyboardKey::KEY_TAB) {
            if window.is_cursor_hidden() {
                window.enable_cursor();
            } else {
                window.disable_cursor();
            }
        }
        if window.is_key_pressed(KeyboardKey::KEY_M) {
            mode = if mode == Mode::Map2D {
                Mode::World3D
            } else {
                Mode::Map2D
            };
        }
        if window.is_key_pressed(KeyboardKey::KEY_N) {
            show_minimap = !show_minimap;
        }

        // El render sólo sabe dibujar Sprites (posición + textura + tamaño),
        // no de IA ni de si un tótem sigue en pie: cada frame se saca una
        // foto de cómo quedó todo después de actualizarlo. Un tótem
        // destruido no produce Sprite (`Totem::sprite` regresa `None`), así
        // que desaparece solo del mundo en cuanto se destruye.
        let enemy_sprites: Vec<Sprite> = enemies.iter().map(|e| e.sprite).collect();
        let totem_sprites: Vec<Sprite> = totems.iter().filter_map(|t| t.sprite()).collect();
        let mut world_sprites = enemy_sprites.clone();
        world_sprites.extend_from_slice(&door_sprites);
        world_sprites.extend_from_slice(&totem_sprites);

        render(
            &mut framebuffer,
            &maze,
            &player,
            mode,
            show_minimap,
            &textures,
            &enemy_sprites,
            &totem_sprites,
            &world_sprites,
        );

        if window.is_key_pressed(KeyboardKey::KEY_F1) {
            framebuffer.render_to_file("maze.png");
            println!(
                "Captura en maze.png | pos ({:.0}, {:.0}) a {:.2} rad",
                player.pos.x, player.pos.y, player.a
            );
        }

        framebuffer.upload();
        {
            let mut d = window.begin_drawing(&thread);
            framebuffer.draw(&mut d);
            draw_hearts_hud(&mut d, width, player.lives, player::START_LIVES);
            draw_totem_counter(&mut d, width, totems.len() - totem::remaining(&totems), totems.len());
            if game_state == GameState::Playing {
                if near_totem {
                    draw_interact_hint(&mut d, width, height);
                }
                if on_locked_goal {
                    draw_locked_door_hint(&mut d, width, totem::remaining(&totems));
                }
            }
            match game_state {
                GameState::Won => draw_end_screen(
                    &mut d,
                    width,
                    height,
                    "GANASTE!",
                    Color::new(255, 220, 60, 255),
                    "Llegaste a la puerta de salida. ESC para salir.",
                ),
                GameState::Lost => draw_end_screen(
                    &mut d,
                    width,
                    height,
                    "PERDISTE!",
                    Color::new(220, 60, 60, 255),
                    "Un enemigo te alcanzo. ESC para salir.",
                ),
                GameState::Playing => {}
            }
        }
    }
}
