use crate::maze::Maze;
use raylib::prelude::*;
use std::f32::consts::PI;

/// Pixeles que avanza el jugador por frame (adelante/atrás o al hacer strafe).
const MOVE_SPEED: f32 = 4.0;
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

/// Convierte movimiento en el espacio local del jugador (`forward`: hacia
/// donde ve; `strafe`: a su derecha) a un desplazamiento (dx, dy) en el
/// mundo, según el ángulo `angle` en el que está viendo. Es una rotación del
/// vector (forward, strafe) por `angle`.
fn move_vector(angle: f32, forward: f32, strafe: f32) -> (f32, f32) {
    let (sin_a, cos_a) = (angle.sin(), angle.cos());
    (
        forward * cos_a - strafe * sin_a,
        forward * sin_a + strafe * cos_a,
    )
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

/// W/S avanzan y retroceden en la dirección de vista, A/D se mueven de lado
/// (strafe) sin girarla; el mouse (eje horizontal) es lo que gira la cámara.
pub fn process_events(
    player: &mut Player,
    window: &RaylibHandle,
    maze: &Maze,
    block_size: usize,
) {
    // Sólo cuenta el movimiento del mouse mientras el cursor está capturado
    // (oculto y centrado por raylib vía disable_cursor): si el usuario lo
    // liberó con TAB para hacer otra cosa, moverlo no debería girar la cámara.
    if window.is_cursor_hidden() {
        let mouse_dx = window.get_mouse_delta().x;
        player.a += mouse_dx * MOUSE_SENSITIVITY;
    }

    // Mantener el ángulo en [0, 2PI) para que no crezca sin límite.
    player.a = player.a.rem_euclid(2.0 * PI);

    let mut forward = 0.0;
    if window.is_key_down(KeyboardKey::KEY_W) {
        forward += MOVE_SPEED;
    }
    if window.is_key_down(KeyboardKey::KEY_S) {
        forward -= MOVE_SPEED;
    }

    let mut strafe = 0.0;
    if window.is_key_down(KeyboardKey::KEY_D) {
        strafe += MOVE_SPEED;
    }
    if window.is_key_down(KeyboardKey::KEY_A) {
        strafe -= MOVE_SPEED;
    }

    if forward == 0.0 && strafe == 0.0 {
        return;
    }

    let (mut dx, mut dy) = move_vector(player.a, forward, strafe);

    // Sin esto, moverse en diagonal (p.ej. W+D) sería más rápido que en línea
    // recta, porque se sumarían dos vectores de magnitud MOVE_SPEED.
    let length = (dx * dx + dy * dy).sqrt();
    if length > MOVE_SPEED {
        let scale = MOVE_SPEED / length;
        dx *= scale;
        dy *= scale;
    }

    // Cada eje se prueba por separado: si uno choca, el otro todavía puede
    // avanzar y el jugador se desliza sobre la pared en vez de trabarse.
    if is_free(maze, player.pos.x + dx, player.pos.y, block_size) {
        player.pos.x += dx;
    }
    if is_free(maze, player.pos.x, player.pos.y + dy, block_size) {
        player.pos.y += dy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strafe_right_is_90_degrees_clockwise_from_facing() {
        // Con y hacia abajo, en un compás N-E-S-O sentido horario, la
        // derecha de quien ve hacia el "este" (angulo 0) es el "sur" (+y);
        // la derecha de quien ve hacia el "sur" (PI/2) es el "oeste" (-x).
        let (dx, dy) = move_vector(0.0, 0.0, MOVE_SPEED);
        assert!(dx.abs() < 1e-4, "dx = {dx}");
        assert!((dy - MOVE_SPEED).abs() < 1e-4, "dy = {dy}");

        let (dx, dy) = move_vector(PI / 2.0, 0.0, MOVE_SPEED);
        assert!((dx + MOVE_SPEED).abs() < 1e-4, "dx = {dx}");
        assert!(dy.abs() < 1e-4, "dy = {dy}");
    }

    #[test]
    fn strafe_left_is_opposite_of_strafe_right() {
        let (rx, ry) = move_vector(0.7, 0.0, MOVE_SPEED);
        let (lx, ly) = move_vector(0.7, 0.0, -MOVE_SPEED);
        assert!((rx + lx).abs() < 1e-4);
        assert!((ry + ly).abs() < 1e-4);
    }

    #[test]
    fn forward_matches_the_viewing_direction() {
        let angle = 0.9_f32;
        let (dx, dy) = move_vector(angle, MOVE_SPEED, 0.0);
        assert!((dx - MOVE_SPEED * angle.cos()).abs() < 1e-4);
        assert!((dy - MOVE_SPEED * angle.sin()).abs() < 1e-4);
    }
}
