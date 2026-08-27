use crate::maze::Maze;
use raylib::prelude::*;
use std::f32::consts::PI;

/// Pixeles que avanza el jugador por frame caminando (sin Shift). Bastante
/// más lento que antes a propósito: así correr se siente como una decisión
/// real, no como el ritmo por defecto.
const WALK_SPEED: f32 = 2.2;
/// Pixeles por frame corriendo (Shift, mientras haya energía).
const RUN_SPEED: f32 = 4.6;
/// Radianes que gira la cámara por cada pixel que se mueve el mouse en horizontal.
const MOUSE_SENSITIVITY: f32 = 0.0035;
/// Radio del jugador para colisiones: evita que el centro se pegue a la pared.
const RADIUS: f32 = 8.0;

/// Corazones con los que arranca el jugador.
pub const START_LIVES: u32 = 3;
/// Frames de invulnerabilidad tras recibir daño (a 60 fps, ~1.2s), para que
/// un enemigo pegado al jugador no le quite las 3 vidas de un solo tirón.
const INVULN_FRAMES: u32 = 72;
/// Cada cuántos frames suena un paso mientras el jugador camina o corre (a
/// 60 fps: ~0.3s caminando, ~0.17s corriendo).
const FOOTSTEP_INTERVAL_WALK: u32 = 18;
const FOOTSTEP_INTERVAL_RUN: u32 = 10;

/// Energía máxima con la que arranca el jugador.
pub const MAX_STAMINA: f32 = 100.0;
/// Energía que se gasta por frame mientras se corre (a 60 fps, unos 2.8s de
/// carrera continua antes de vaciarse del todo).
const STAMINA_DRAIN_PER_FRAME: f32 = 0.6;
/// Energía que se recupera por frame mientras no se corre (caminando o
/// parado): más lento que gastarla, para que correr sin parar tenga costo.
const STAMINA_REGEN_PER_FRAME: f32 = 0.3;

/// El jugador es el punto de vista del mundo: dónde está y hacia dónde ve.
pub struct Player {
    /// Posición en pixeles dentro del framebuffer, no en celdas.
    pub pos: Vector2,
    /// Ángulo de vista en radianes (hacia dónde apunta la cabeza).
    pub a: f32,
    /// Campo de visión en radianes, se usará al proyectar en 3D.
    pub fov: f32,
    /// Corazones que le quedan. Llega a 0 -> game over.
    pub lives: u32,
    /// Frames que faltan para poder volver a recibir daño.
    invuln_timer: u32,
    /// Si el jugador se desplazó de verdad en el último frame (no sólo tenía
    /// una tecla de movimiento apretada: pudo estar bloqueado por una pared).
    moving: bool,
    /// Si ese desplazamiento fue corriendo (Shift) en vez de caminando.
    running: bool,
    /// Frames que faltan para el siguiente sonido de paso.
    footstep_timer: u32,
    /// Energía para correr. Se gasta corriendo, se recupera caminando o
    /// parado; en 0 ya no se puede correr aunque se tenga Shift apretado.
    pub stamina: f32,
}

impl Player {
    pub fn new(pos: Vector2, a: f32, fov: f32) -> Self {
        Player {
            pos,
            a,
            fov,
            lives: START_LIVES,
            invuln_timer: 0,
            moving: false,
            running: false,
            footstep_timer: 0,
            stamina: MAX_STAMINA,
        }
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

    /// Hace avanzar el tiempo de invulnerabilidad. Se llama una vez por
    /// frame mientras la partida sigue en curso.
    pub fn tick(&mut self) {
        if self.invuln_timer > 0 {
            self.invuln_timer -= 1;
        }
    }

    pub fn is_invulnerable(&self) -> bool {
        self.invuln_timer > 0
    }

    /// Resta un corazón, salvo que el jugador siga invulnerable por un golpe
    /// reciente (o ya esté en 0). No hace nada si no hay daño que aplicar.
    pub fn take_damage(&mut self) {
        if self.is_invulnerable() || self.lives == 0 {
            return;
        }
        self.lives -= 1;
        self.invuln_timer = INVULN_FRAMES;
    }

    /// Si toca reproducir un sonido de paso este frame. Sólo cuenta mientras
    /// el jugador se está desplazando de verdad; en cuanto se detiene, el
    /// contador se reinicia para que el primer paso al retomar la marcha
    /// suene de inmediato en vez de heredar el tiempo que ya llevaba corrido.
    /// Corriendo, los pasos suenan más seguido.
    pub fn should_play_footstep(&mut self) -> bool {
        if !self.moving {
            self.footstep_timer = 0;
            return false;
        }
        if self.footstep_timer == 0 {
            self.footstep_timer = if self.running {
                FOOTSTEP_INTERVAL_RUN
            } else {
                FOOTSTEP_INTERVAL_WALK
            };
            true
        } else {
            self.footstep_timer -= 1;
            false
        }
    }
}

/// Gasto o recuperación de energía de un frame: baja mientras se corre, sube
/// mientras no (caminando o parado), sin salirse de [0, MAX_STAMINA]. Vive
/// aparte de `process_events` (que necesita una ventana real) para poder
/// probarlo directo.
fn apply_stamina(stamina: f32, running: bool) -> f32 {
    if running {
        (stamina - STAMINA_DRAIN_PER_FRAME).max(0.0)
    } else {
        (stamina + STAMINA_REGEN_PER_FRAME).min(MAX_STAMINA)
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

    // Vector de intención sin escalar todavía (+1/-1 por eje): la velocidad
    // real (caminar o correr) se decide después, una vez que sabemos si de
    // verdad hay hacia dónde moverse.
    let mut forward = 0.0;
    if window.is_key_down(KeyboardKey::KEY_W) {
        forward += 1.0;
    }
    if window.is_key_down(KeyboardKey::KEY_S) {
        forward -= 1.0;
    }

    let mut strafe = 0.0;
    if window.is_key_down(KeyboardKey::KEY_D) {
        strafe += 1.0;
    }
    if window.is_key_down(KeyboardKey::KEY_A) {
        strafe -= 1.0;
    }

    if forward == 0.0 && strafe == 0.0 {
        player.moving = false;
        player.running = false;
        // Parado también recupera energía, igual que caminando.
        player.stamina = apply_stamina(player.stamina, false);
        return;
    }

    let wants_to_run = player.stamina > 0.0
        && (window.is_key_down(KeyboardKey::KEY_LEFT_SHIFT)
            || window.is_key_down(KeyboardKey::KEY_RIGHT_SHIFT));
    let speed = if wants_to_run { RUN_SPEED } else { WALK_SPEED };
    player.running = wants_to_run;
    player.stamina = apply_stamina(player.stamina, wants_to_run);

    let (mut dx, mut dy) = move_vector(player.a, forward * speed, strafe * speed);

    // Sin esto, moverse en diagonal (p.ej. W+D) sería más rápido que en línea
    // recta, porque se sumarían dos vectores de magnitud `speed`.
    let length = (dx * dx + dy * dy).sqrt();
    if length > speed {
        let scale = speed / length;
        dx *= scale;
        dy *= scale;
    }

    let before = player.pos;

    // Cada eje se prueba por separado: si uno choca, el otro todavía puede
    // avanzar y el jugador se desliza sobre la pared en vez de trabarse.
    if maze.is_free(player.pos.x + dx, player.pos.y, block_size, RADIUS) {
        player.pos.x += dx;
    }
    if maze.is_free(player.pos.x, player.pos.y + dy, block_size, RADIUS) {
        player.pos.y += dy;
    }

    // "Moviéndose" quiere decir que de verdad cambió de lugar, no sólo que
    // tenía una tecla apretada: si está bloqueado contra una pared no debería
    // sonar como si estuviera caminando.
    player.moving = player.pos.x != before.x || player.pos.y != before.y;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strafe_right_is_90_degrees_clockwise_from_facing() {
        // Con y hacia abajo, en un compás N-E-S-O sentido horario, la
        // derecha de quien ve hacia el "este" (angulo 0) es el "sur" (+y);
        // la derecha de quien ve hacia el "sur" (PI/2) es el "oeste" (-x).
        let (dx, dy) = move_vector(0.0, 0.0, WALK_SPEED);
        assert!(dx.abs() < 1e-4, "dx = {dx}");
        assert!((dy - WALK_SPEED).abs() < 1e-4, "dy = {dy}");

        let (dx, dy) = move_vector(PI / 2.0, 0.0, WALK_SPEED);
        assert!((dx + WALK_SPEED).abs() < 1e-4, "dx = {dx}");
        assert!(dy.abs() < 1e-4, "dy = {dy}");
    }

    #[test]
    fn strafe_left_is_opposite_of_strafe_right() {
        let (rx, ry) = move_vector(0.7, 0.0, WALK_SPEED);
        let (lx, ly) = move_vector(0.7, 0.0, -WALK_SPEED);
        assert!((rx + lx).abs() < 1e-4);
        assert!((ry + ly).abs() < 1e-4);
    }

    #[test]
    fn forward_matches_the_viewing_direction() {
        let angle = 0.9_f32;
        let (dx, dy) = move_vector(angle, WALK_SPEED, 0.0);
        assert!((dx - WALK_SPEED * angle.cos()).abs() < 1e-4);
        assert!((dy - WALK_SPEED * angle.sin()).abs() < 1e-4);
    }

    #[test]
    fn taking_damage_costs_one_life_and_grants_invulnerability() {
        let mut player = Player::new(Vector2::new(0.0, 0.0), 0.0, PI / 3.0);
        assert_eq!(player.lives, START_LIVES);
        assert!(!player.is_invulnerable());

        player.take_damage();
        assert_eq!(player.lives, START_LIVES - 1);
        assert!(player.is_invulnerable());
    }

    #[test]
    fn cannot_lose_a_second_life_while_still_invulnerable() {
        let mut player = Player::new(Vector2::new(0.0, 0.0), 0.0, PI / 3.0);
        player.take_damage();
        let lives_after_first_hit = player.lives;

        // Un enemigo pegado al jugador varios frames seguidos no debería
        // seguir quitando vidas mientras dura la invulnerabilidad.
        for _ in 0..INVULN_FRAMES - 1 {
            player.take_damage();
            player.tick();
        }

        assert_eq!(player.lives, lives_after_first_hit);
    }

    #[test]
    fn can_lose_a_life_again_once_invulnerability_runs_out() {
        let mut player = Player::new(Vector2::new(0.0, 0.0), 0.0, PI / 3.0);
        player.take_damage();

        for _ in 0..INVULN_FRAMES {
            player.tick();
        }
        assert!(!player.is_invulnerable());

        player.take_damage();
        assert_eq!(player.lives, START_LIVES - 2);
    }

    #[test]
    fn lives_do_not_go_below_zero() {
        let mut player = Player::new(Vector2::new(0.0, 0.0), 0.0, PI / 3.0);
        for _ in 0..10 {
            player.take_damage();
            for _ in 0..INVULN_FRAMES {
                player.tick();
            }
        }
        assert_eq!(player.lives, 0);
    }

    #[test]
    fn no_footsteps_while_standing_still() {
        let mut player = Player::new(Vector2::new(0.0, 0.0), 0.0, PI / 3.0);
        for _ in 0..FOOTSTEP_INTERVAL_WALK * 2 {
            assert!(!player.should_play_footstep());
        }
    }

    #[test]
    fn footsteps_repeat_at_a_fixed_interval_while_moving() {
        let mut player = Player::new(Vector2::new(0.0, 0.0), 0.0, PI / 3.0);
        player.moving = true;

        // El primer paso suena de inmediato al arrancar a caminar.
        assert!(player.should_play_footstep());

        let mut steps = 0;
        for _ in 0..FOOTSTEP_INTERVAL_WALK * 3 {
            if player.should_play_footstep() {
                steps += 1;
            }
        }
        // Cada paso siguiente tarda FOOTSTEP_INTERVAL_WALK + 1 llamadas (el
        // intervalo de espera más la llamada que sí suena): en 3 intervalos
        // sólo alcanza a caer 2 veces más.
        assert_eq!(steps, 2);
    }

    #[test]
    fn stopping_resets_the_footstep_countdown() {
        let mut player = Player::new(Vector2::new(0.0, 0.0), 0.0, PI / 3.0);
        player.moving = true;
        player.should_play_footstep(); // primer paso, arranca el conteo
        player.should_play_footstep();
        player.should_play_footstep();

        player.moving = false;
        player.should_play_footstep(); // se detiene: reinicia el contador

        player.moving = true;
        // Debería sonar de inmediato otra vez, no a mitad de un intervalo
        // heredado de antes de detenerse.
        assert!(player.should_play_footstep());
    }

    #[test]
    fn running_drains_stamina_and_walking_regenerates_it() {
        let after_running = apply_stamina(MAX_STAMINA, true);
        assert!(after_running < MAX_STAMINA);

        let after_walking = apply_stamina(after_running, false);
        assert!(after_walking > after_running);
    }

    #[test]
    fn stamina_stays_within_bounds() {
        let mut stamina = 0.0;
        for _ in 0..10 {
            stamina = apply_stamina(stamina, true);
        }
        assert_eq!(stamina, 0.0, "no debería bajar de 0");

        let mut stamina = MAX_STAMINA;
        for _ in 0..10 {
            stamina = apply_stamina(stamina, false);
        }
        assert_eq!(stamina, MAX_STAMINA, "no debería pasarse del máximo");
    }

    #[test]
    fn running_is_faster_than_walking() {
        assert!(RUN_SPEED > WALK_SPEED);
    }
}
