use crate::maze::Maze;
use raylib::prelude::*;
use std::f32::consts::PI;

/// Pixeles que avanza el jugador por frame.
const MOVE_SPEED: f32 = 4.0;
/// Radianes que gira la cámara por frame con el teclado.
const ROTATION_SPEED: f32 = PI / 60.0;
/// Radianes que gira la cámara por cada pixel que se mueve el mouse en horizontal.
const MOUSE_SENSITIVITY: f32 = 0.0035;
/// Radio del jugador para colisiones: evita que el centro se pegue a la pared.
const RADIUS: f32 = 8.0;

/// El jugador es el punto de vista del mundo: dónde está y hacia dónde ve.
pub struct Player {
    /// Posición en pixeles dentro del framebuffer, no en celdas.
    pub pos: Vector2,
    /// Ángulo de vista en radianes (hacia dónde apunta la cabeza).
    pub a: f32,
    /// Campo de visión en radianes, se usará al proyectar en 3D.
    pub fov: f32,
}

impl Player {
    pub fn new(pos: Vector2, a: f32, fov: f32) -> Self {
        Player { pos, a, fov }
    }

    /// Construye al jugador al centro de la celda (cell_x, cell_y) del mapa.
    pub fn at_cell(cell_x: usize, cell_y: usize, block_size: usize, a: f32, fov: f32) -> Self {
        let half = block_size as f32 / 2.0;
        Player::new(
            Vector2::new(
                cell_x as f32 * block_size as f32 + half,
                cell_y as f32 * block_size as f32 + half,
            ),
            a,
            fov,
        )
    }
}

/// ¿Cabe el jugador con centro en (x, y)? Revisa las cuatro esquinas de su
/// caja, así no se mete de lado a una pared.
fn is_free(maze: &Maze, x: f32, y: f32, block_size: usize) -> bool {
    for (dx, dy) in [
        (-RADIUS, -RADIUS),
        (RADIUS, -RADIUS),
        (-RADIUS, RADIUS),
        (RADIUS, RADIUS),
    ] {
        let px = x + dx;
        let py = y + dy;
        if px < 0.0 || py < 0.0 {
            return false;
        }
        if maze.is_wall(px as usize / block_size, py as usize / block_size) {
            return false;
        }
    }
    true
}

/// W/S avanzan y retroceden en la dirección de vista; A/D y el mouse (eje
/// horizontal) giran la cámara.
pub fn process_events(
    player: &mut Player,
    window: &RaylibHandle,
    maze: &Maze,
    block_size: usize,
) {
    if window.is_key_down(KeyboardKey::KEY_A) {
        player.a -= ROTATION_SPEED;
    }
    if window.is_key_down(KeyboardKey::KEY_D) {
        player.a += ROTATION_SPEED;
    }

    // Sólo cuenta el movimiento del mouse mientras el cursor está capturado
    // (oculto y centrado por raylib vía disable_cursor): si el usuario lo
    // liberó con TAB para hacer otra cosa, moverlo no debería girar la cámara.
    if window.is_cursor_hidden() {
        let mouse_dx = window.get_mouse_delta().x;
        player.a += mouse_dx * MOUSE_SENSITIVITY;
    }

    // Mantener el ángulo en [0, 2PI) para que no crezca sin límite.
    player.a = player.a.rem_euclid(2.0 * PI);

    let mut step = 0.0;
    if window.is_key_down(KeyboardKey::KEY_W) {
        step += MOVE_SPEED;
    }
    if window.is_key_down(KeyboardKey::KEY_S) {
        step -= MOVE_SPEED;
    }
    if step == 0.0 {
        return;
    }

    let dx = step * player.a.cos();
    let dy = step * player.a.sin();

    // Cada eje se prueba por separado: si uno choca, el otro todavía puede
    // avanzar y el jugador se desliza sobre la pared en vez de trabarse.
    if is_free(maze, player.pos.x + dx, player.pos.y, block_size) {
        player.pos.x += dx;
    }
    if is_free(maze, player.pos.x, player.pos.y + dy, block_size) {
        player.pos.y += dy;
    }
}
