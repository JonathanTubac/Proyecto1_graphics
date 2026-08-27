use crate::maze::Maze;
use crate::player::Player;
use crate::sprites::Sprite;
use rand::Rng;
use raylib::prelude::*;
use std::f32::consts::{PI, TAU};

/// Qué tan lejos puede ver un enemigo antes de perder al jugador de vista.
const SIGHT_RANGE: f32 = 220.0;
/// Ancho total del cono de visión (a cada lado del "facing" hay la mitad).
/// ~103°: bastante amplio para que un guardia se sienta atento, sin ser
/// omnisciente.
const SIGHT_FOV: f32 = 1.8;
/// Pixeles por frame que avanza un enemigo mientras persigue.
const CHASE_SPEED: f32 = 1.8;
/// Pixeles por frame que camina un enemigo mientras patrulla solo. Más
/// lento que perseguir, para que se note la diferencia entre "va de ronda"
/// y "ya me vio".
const WANDER_SPEED: f32 = 0.9;
/// Cada cuántos frames (a 60 fps, ~1.5s) un enemigo que patrulla cambia de
/// dirección aunque no haya chocado con nada, para que no camine derecho
/// por el mismo pasillo para siempre.
const WANDER_CHANGE_INTERVAL: u32 = 90;
/// No se acerca más que esto al jugador, para no superponer su sprite con
/// la cámara.
const STOP_DISTANCE: f32 = 24.0;
/// Qué tan cerca tiene que estar un enemigo para hacerle daño al jugador. Un
/// poco más que `STOP_DISTANCE`, así que en la práctica pasa en cuanto un
/// enemigo que persigue alcanza al jugador y se para.
const DAMAGE_RANGE: f32 = 26.0;
/// Radio de colisión del enemigo contra las paredes (mismo criterio que el jugador).
const RADIUS: f32 = 8.0;
/// Cada cuántos frames suena un paso del enemigo mientras se mueve, a
/// velocidad base (a 60 fps, ~0.35s entre pasos). Se acorta según qué tan
/// rápido esté yendo en este momento.
const BASE_FOOTSTEP_INTERVAL: f32 = 21.0;
/// Cuántos frames sigue un enemigo "alertado" yendo directo hacia el
/// jugador sin necesitar verlo, después de que se rompe un tótem: como si
/// hubiera oído el estruendo y supiera más o menos dónde buscar. A 60fps,
/// ~10s. Si el jugador se esconde (`Player::hidden`), `can_see_player`
/// sigue devolviendo falso y `damage_player_if_close` no le hace nada, así
/// que la alerta por sí sola no basta para encontrarlo: sólo lo acerca.
const ALERT_DURATION: u32 = 600;

/// Caracteres de textura para cada combinación de vista (de frente / de
/// espaldas) y pose (quieto, o cuadro A/B de la animación de correr).
const TEX_FRONT_IDLE: char = 'e';
const TEX_BACK_IDLE: char = 'b';
const TEX_FRONT_RUN_A: char = '1';
const TEX_FRONT_RUN_B: char = '2';
const TEX_BACK_RUN_A: char = '3';
const TEX_BACK_RUN_B: char = '4';

/// Si un enemigo ve al jugador ahora mismo o no. Se recalcula cada frame a
/// partir de la visión: no hay memoria, en cuanto lo pierde de vista deja de
/// perseguir y se queda quieto donde está.
#[derive(Clone, Copy, PartialEq)]
enum State {
    Idle,
    Chasing,
}

/// Un enemigo: dónde está, hacia dónde "mira" (su cono de visión) y si
/// ahorita está persiguiendo al jugador. Produce un `Sprite` para que
/// `crate::sprites` lo dibuje; no sabe nada de texturizado ni de billboards.
pub struct Enemy {
    pub sprite: Sprite,
    /// Dirección en la que "vigila", en radianes. Mientras persigue, se
    /// actualiza para apuntar siempre al jugador; mientras patrulla, es la
    /// misma dirección en la que está caminando.
    facing: f32,
    state: State,
    /// Dirección de la ronda actual (`Idle`), en radianes.
    wander_dir: f32,
    /// Frames que faltan para elegir una nueva dirección de ronda.
    wander_timer: u32,
    /// Frames que faltan para el siguiente sonido de paso.
    footstep_timer: u32,
    /// Si toca reproducir un sonido de paso justo este frame.
    footstep_due: bool,
    /// Cuadro de la animación de correr (alterna en cada paso; ver
    /// `sprite_for_viewer`).
    anim_frame: bool,
    /// Frames que le quedan de perseguir al jugador a ciegas (sin verlo)
    /// tras una alerta (ver `alert_all`). 0 = no está alertado.
    alert_timer: u32,
}

impl Enemy {
    fn new(pos: Vector2, texture: char, size: f32, facing: f32) -> Self {
        Enemy {
            sprite: Sprite::new(pos, texture, size),
            facing,
            state: State::Idle,
            wander_dir: facing,
            wander_timer: 0,
            footstep_timer: 0,
            footstep_due: false,
            anim_frame: false,
            alert_timer: 0,
        }
    }

    pub fn pos(&self) -> Vector2 {
        self.sprite.pos
    }

    /// Si está persiguiendo al jugador ahora mismo (por si más adelante se
    /// quiere, por ejemplo, cambiarle el color o la textura al perseguir).
    #[allow(dead_code)]
    pub fn is_chasing(&self) -> bool {
        self.state == State::Chasing
    }

    /// Si toca reproducir un sonido de paso de este enemigo justo este
    /// frame. Se recalcula en cada `update_enemies`, así que sólo hay que
    /// leerlo justo después de llamarla.
    pub fn wants_footstep(&self) -> bool {
        self.footstep_due
    }

    /// Sprite a dibujar este frame. Elige entre vista de frente o de
    /// espaldas comparando hacia dónde "mira" el enemigo contra hacia dónde
    /// queda `viewer` (el jugador) respecto a él; si está persiguiendo,
    /// además alterna dos cuadros de carrera al ritmo de sus propios pasos
    /// (el mismo `footstep_timer` que dispara el sonido).
    pub fn sprite_for_viewer(&self, viewer: Vector2) -> Sprite {
        let dx = viewer.x - self.pos().x;
        let dy = viewer.y - self.pos().y;
        let angle_to_viewer = dy.atan2(dx);
        let mut diff = angle_to_viewer - self.facing;
        diff = (diff + PI).rem_euclid(2.0 * PI) - PI;
        // Si el enemigo mira hacia donde está el jugador, el jugador le ve
        // el frente; si mira para el otro lado, le ve la espalda. No hay
        // vista de perfil: a los 90° se parte la diferencia.
        let front = diff.abs() < PI / 2.0;

        let texture = match (self.state, front, self.anim_frame) {
            (State::Idle, true, _) => TEX_FRONT_IDLE,
            (State::Idle, false, _) => TEX_BACK_IDLE,
            (State::Chasing, true, false) => TEX_FRONT_RUN_A,
            (State::Chasing, true, true) => TEX_FRONT_RUN_B,
            (State::Chasing, false, false) => TEX_BACK_RUN_A,
            (State::Chasing, false, true) => TEX_BACK_RUN_B,
        };

        let mut sprite = self.sprite;
        sprite.texture = texture;
        sprite
    }
}

/// Busca en el laberinto las celdas marcadas con `marker` y crea un enemigo
/// en cada una, con textura `texture` y vigilando inicialmente hacia
/// `initial_facing` (en radianes) hasta que vean al jugador.
pub fn spawn_from_maze(
    maze: &Maze,
    block_size: usize,
    marker: char,
    texture: char,
    initial_facing: f32,
) -> Vec<Enemy> {
    let half = block_size as f32 / 2.0;
    let mut rng = rand::thread_rng();
    maze.find_all(marker)
        .into_iter()
        .map(|(x, y)| {
            let pos = Vector2::new(
                x as f32 * block_size as f32 + half,
                y as f32 * block_size as f32 + half,
            );
            let mut enemy = Enemy::new(pos, texture, block_size as f32, initial_facing);
            // Arranca cada enemigo con un tiempo distinto para su primer
            // cambio de dirección, para que no patrullen todos sincronizados.
            enemy.wander_timer = rng.gen_range(0..WANDER_CHANGE_INTERVAL);
            enemy
        })
        .collect()
}

/// ¿Puede el enemigo ver al jugador desde donde está? Necesita que esté
/// dentro del rango y del cono de visión, y que no haya una pared de por
/// medio. Escondido en un locker (`Player::hidden`), nunca se le ve, sin
/// importar qué tan cerca o de frente esté.
fn can_see_player(enemy: &Enemy, player: &Player, maze: &Maze, block_size: usize) -> bool {
    if player.hidden {
        return false;
    }

    let dx = player.pos.x - enemy.pos().x;
    let dy = player.pos.y - enemy.pos().y;
    let distance = (dx * dx + dy * dy).sqrt();
    if distance > SIGHT_RANGE {
        return false;
    }

    // A distancia casi nula el ángulo no está bien definido (y da igual: ya
    // lo tiene encima).
    if distance >= 1.0 {
        let angle_to_player = dy.atan2(dx);
        let mut angle_diff = angle_to_player - enemy.facing;
        angle_diff = (angle_diff + PI).rem_euclid(2.0 * PI) - PI;
        if angle_diff.abs() > SIGHT_FOV / 2.0 {
            return false;
        }
    }

    maze.line_of_sight(
        (enemy.pos().x, enemy.pos().y),
        (player.pos.x, player.pos.y),
        block_size,
    )
}

/// Si algún enemigo está lo bastante cerca, le quita una vida al jugador.
/// `Player::take_damage` ya se encarga de no descontar de más mientras siga
/// invulnerable, así que aquí basta con detectar "hay alguien encima".
/// Escondido en un locker (`Player::hidden`), está a salvo aunque el
/// enemigo esté parado justo afuera: es el sentido de esconderse.
pub fn damage_player_if_close(enemies: &[Enemy], player: &mut Player) {
    if player.hidden {
        return;
    }

    let close = enemies.iter().any(|enemy| {
        let dx = player.pos.x - enemy.pos().x;
        let dy = player.pos.y - enemy.pos().y;
        (dx * dx + dy * dy).sqrt() < DAMAGE_RANGE
    });
    if close {
        player.take_damage();
    }
}

/// Actualiza la visión, el estado y la posición de todos los enemigos para
/// este frame: persiguen si ven al jugador, patrullan solos si no.
///
/// `speed_multiplier` escala tanto la velocidad de persecución como la de
/// ronda: en este laberinto se usa para que el único enemigo del nivel se
/// vuelva más rápido con cada tótem que se destruye, así que se recalcula
/// afuera (ver `crate::main`) y se pasa fresco cada frame.
pub fn update_enemies(
    enemies: &mut [Enemy],
    player: &Player,
    maze: &Maze,
    block_size: usize,
    speed_multiplier: f32,
) {
    let mut rng = rand::thread_rng();
    for enemy in enemies {
        if enemy.alert_timer > 0 {
            enemy.alert_timer -= 1;
        }

        // Persigue si lo ve, o si sigue alertado por un tótem roto aunque
        // no lo vea todavía: ambos casos usan el mismo movimiento (ir
        // directo hacia el jugador), la diferencia es sólo por qué.
        let seen = can_see_player(enemy, player, maze, block_size);
        let hunting = enemy.alert_timer > 0;
        enemy.state = if seen || hunting {
            State::Chasing
        } else {
            State::Idle
        };

        let moved = match enemy.state {
            State::Chasing => chase_player(enemy, player, maze, block_size, speed_multiplier),
            State::Idle => wander(enemy, maze, block_size, speed_multiplier, &mut rng),
        };

        update_footstep_timer(enemy, moved, speed_multiplier);
    }
}

/// Pone a todos los enemigos en alerta: durante `ALERT_DURATION` frames van
/// directo hacia donde está el jugador aunque no lo vean, como si hubieran
/// oído el tótem romperse. Se llama cada vez que se destruye alguno, así
/// que romper varios seguidos mantiene la persecución viva en vez de
/// dejarla expirar a medio camino.
pub fn alert_all(enemies: &mut [Enemy]) {
    for enemy in enemies {
        enemy.alert_timer = ALERT_DURATION;
    }
}

/// Cada vez que el enemigo avanza de verdad, cuenta hacia el siguiente
/// sonido de paso; entre más rápido vaya, más seguido suenan. Si no se movió
/// este frame (chocó, o está parado a `STOP_DISTANCE` del jugador), no debe
/// sonar ni heredar un conteo a medias de la próxima vez que retome el paso.
fn update_footstep_timer(enemy: &mut Enemy, moved: bool, speed_multiplier: f32) {
    if !moved {
        enemy.footstep_timer = 0;
        enemy.footstep_due = false;
        return;
    }

    if enemy.footstep_timer == 0 {
        enemy.footstep_due = true;
        enemy.anim_frame = !enemy.anim_frame;
        enemy.footstep_timer = (BASE_FOOTSTEP_INTERVAL / speed_multiplier.max(0.01)) as u32;
    } else {
        enemy.footstep_due = false;
        enemy.footstep_timer -= 1;
    }
}

/// Camina en línea recta hacia el jugador, deslizándose sobre las paredes
/// igual que el jugador, y se para a `STOP_DISTANCE` para no superponerse
/// con la cámara. Regresa si avanzó de verdad este frame.
fn chase_player(
    enemy: &mut Enemy,
    player: &Player,
    maze: &Maze,
    block_size: usize,
    speed_multiplier: f32,
) -> bool {
    let dx = player.pos.x - enemy.pos().x;
    let dy = player.pos.y - enemy.pos().y;
    let distance = (dx * dx + dy * dy).sqrt();

    // Sigue mirando hacia el jugador aunque ya no avance más: es lo que
    // hace que el cono de visión "lo siga teniendo encima" mientras esté
    // cerca, en vez de perderlo de vista por quedarse mirando fijo.
    enemy.facing = dy.atan2(dx);
    // Si lo pierde de vista y vuelve a patrullar, que siga hacia donde
    // estaba yendo en vez de saltar a una dirección al azar de golpe.
    enemy.wander_dir = enemy.facing;

    if distance < STOP_DISTANCE {
        return false; // ya está lo bastante cerca, no hace falta seguir avanzando
    }

    let speed = CHASE_SPEED * speed_multiplier;
    let (step_x, step_y) = (dx / distance * speed, dy / distance * speed);
    move_with_wall_slide(enemy, maze, block_size, step_x, step_y)
}

/// Camina solo en una dirección hasta chocar con una pared o hasta que se
/// cumpla el tiempo de ronda, momento en el que elige una dirección nueva al
/// azar. Es un recorrido simple ("rebota" en las paredes), no un patrullaje
/// por puntos fijos, pero alcanza para que el laberinto se sienta habitado.
/// Regresa si avanzó de verdad este frame.
fn wander(
    enemy: &mut Enemy,
    maze: &Maze,
    block_size: usize,
    speed_multiplier: f32,
    rng: &mut impl Rng,
) -> bool {
    if enemy.wander_timer == 0 {
        enemy.wander_dir = rng.gen_range(0.0..TAU);
        enemy.wander_timer = WANDER_CHANGE_INTERVAL;
    } else {
        enemy.wander_timer -= 1;
    }
    enemy.facing = enemy.wander_dir;

    let speed = WANDER_SPEED * speed_multiplier;
    let step_x = enemy.wander_dir.cos() * speed;
    let step_y = enemy.wander_dir.sin() * speed;
    let moved = move_with_wall_slide(enemy, maze, block_size, step_x, step_y);

    // Chocó de lleno (no avanzó en ningún eje): no tiene caso esperar al
    // timer, mejor elegir otra dirección ya para no quedarse vibrando
    // contra la pared.
    if !moved {
        enemy.wander_timer = 0;
    }

    moved
}

/// Mueve al enemigo por (step_x, step_y), probando cada eje por separado:
/// si uno choca, el otro todavía puede avanzar y se desliza sobre la pared
/// en vez de trabarse en una esquina. Regresa si avanzó en algún eje.
fn move_with_wall_slide(
    enemy: &mut Enemy,
    maze: &Maze,
    block_size: usize,
    step_x: f32,
    step_y: f32,
) -> bool {
    let pos = &mut enemy.sprite.pos;
    let mut moved = false;

    if maze.is_free(pos.x + step_x, pos.y, block_size, RADIUS) {
        pos.x += step_x;
        moved = true;
    }
    if maze.is_free(pos.x, pos.y + step_y, block_size, RADIUS) {
        pos.y += step_y;
        moved = true;
    }

    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// Cuarto rectangular abierto de `w` x `h` celdas, con borde de pared.
    fn open_room(w: usize, h: usize) -> Maze {
        let mut cells = vec![vec!['+'; w]; h];
        for row in cells.iter_mut().take(h - 1).skip(1) {
            for cell in row.iter_mut().take(w - 1).skip(1) {
                *cell = ' ';
            }
        }
        Maze::new(cells)
    }

    #[test]
    fn sees_player_directly_ahead_within_range() {
        let maze = open_room(10, 5);
        let block = 20;
        let enemy = Enemy::new(Vector2::new(40.0, 50.0), 'e', block as f32, 0.0); // mira al este
        let player = Player::new(Vector2::new(100.0, 50.0), 0.0, PI / 3.0);
        assert!(can_see_player(&enemy, &player, &maze, block));
    }

    #[test]
    fn does_not_see_player_directly_behind_it() {
        let maze = open_room(10, 5);
        let block = 20;
        let enemy = Enemy::new(Vector2::new(100.0, 50.0), 'e', block as f32, 0.0); // mira al este
        let player = Player::new(Vector2::new(40.0, 50.0), 0.0, PI / 3.0); // detrás, al oeste
        assert!(!can_see_player(&enemy, &player, &maze, block));
    }

    #[test]
    fn does_not_see_player_beyond_sight_range() {
        let maze = open_room(30, 5);
        let block = 20;
        let enemy = Enemy::new(Vector2::new(40.0, 50.0), 'e', block as f32, 0.0);
        let far_x = 40.0 + SIGHT_RANGE + 40.0;
        let player = Player::new(Vector2::new(far_x, 50.0), 0.0, PI / 3.0);
        assert!(!can_see_player(&enemy, &player, &maze, block));
    }

    #[test]
    fn does_not_see_player_through_a_wall() {
        // Dos cuartos (celdas 1-3 y 5-9) separados por una pared en x=4.
        let cells: Vec<Vec<char>> = ["+++++++++++", "+   +     +", "+   +     +", "+++++++++++"]
            .iter()
            .map(|r| r.chars().collect())
            .collect();
        let maze = Maze::new(cells);
        let block = 20;
        let enemy = Enemy::new(Vector2::new(40.0, 30.0), 'e', block as f32, 0.0);
        let player = Player::new(Vector2::new(140.0, 30.0), 0.0, PI / 3.0);
        assert!(!can_see_player(&enemy, &player, &maze, block));
    }

    #[test]
    fn wandering_enemy_moves_around_an_open_room() {
        let maze = open_room(15, 15);
        let block = 20;
        let mut enemy = Enemy::new(Vector2::new(150.0, 150.0), 'e', block as f32, 0.0);
        let start = enemy.pos();

        // Semilla fija: la prueba no depende de qué tan "con suerte" salga
        // el azar en una corrida particular.
        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..30 {
            wander(&mut enemy, &maze, block, 1.0, &mut rng);
        }

        assert!(
            (enemy.pos() - start).length() > 0.0,
            "el enemigo no se movió nada tras 30 pasos de ronda"
        );
    }

    #[test]
    fn wandering_enemy_stays_inside_the_walls() {
        let maze = open_room(15, 15);
        let block = 20;
        let mut enemy = Enemy::new(Vector2::new(150.0, 150.0), 'e', block as f32, 0.0);
        let mut rng = StdRng::seed_from_u64(7);

        for _ in 0..300 {
            wander(&mut enemy, &maze, block, 1.0, &mut rng);
            assert!(
                maze.is_free(enemy.pos().x, enemy.pos().y, block, 0.0),
                "el enemigo terminó dentro de una pared: {:?}",
                enemy.pos()
            );
        }
    }

    #[test]
    fn hitting_a_wall_forces_a_new_direction_next_tick() {
        let maze = open_room(15, 15);
        let block = 20;
        // Pegado a la pared izquierda (x=20 es el borde de la celda 1),
        // mirando derecho hacia ella: el primer paso debería chocar.
        let mut enemy = Enemy::new(Vector2::new(21.0, 150.0), 'e', block as f32, PI);
        enemy.wander_dir = PI; // hacia el oeste, directo a la pared
        enemy.wander_timer = WANDER_CHANGE_INTERVAL; // que no se deba a que ya le tocaba cambiar

        let mut rng = StdRng::seed_from_u64(7);
        wander(&mut enemy, &maze, block, 1.0, &mut rng);

        assert_eq!(enemy.wander_timer, 0, "debería forzar un cambio de dirección al chocar");
    }

    #[test]
    fn player_takes_damage_when_an_enemy_is_close() {
        let enemies = vec![Enemy::new(Vector2::new(0.0, 0.0), 'e', 20.0, 0.0)];
        let mut player = Player::new(Vector2::new(10.0, 0.0), 0.0, PI / 3.0);

        damage_player_if_close(&enemies, &mut player);

        assert_eq!(player.lives, crate::player::START_LIVES - 1);
    }

    #[test]
    fn player_takes_no_damage_when_every_enemy_is_far() {
        let enemies = vec![Enemy::new(Vector2::new(0.0, 0.0), 'e', 20.0, 0.0)];
        let mut player = Player::new(Vector2::new(1000.0, 0.0), 0.0, PI / 3.0);

        damage_player_if_close(&enemies, &mut player);

        assert_eq!(player.lives, crate::player::START_LIVES);
    }

    #[test]
    fn hidden_player_is_never_seen() {
        let maze = open_room(10, 5);
        let block = 20;
        let enemy = Enemy::new(Vector2::new(40.0, 50.0), 'e', block as f32, 0.0); // mira al este, jugador justo enfrente
        let mut player = Player::new(Vector2::new(100.0, 50.0), 0.0, PI / 3.0);
        player.hidden = true;

        assert!(!can_see_player(&enemy, &player, &maze, block));
    }

    #[test]
    fn hidden_player_takes_no_damage_even_when_close() {
        let enemies = vec![Enemy::new(Vector2::new(0.0, 0.0), 'e', 20.0, 0.0)];
        let mut player = Player::new(Vector2::new(10.0, 0.0), 0.0, PI / 3.0);
        player.hidden = true;

        damage_player_if_close(&enemies, &mut player);

        assert_eq!(player.lives, crate::player::START_LIVES);
    }

    #[test]
    fn alert_makes_enemy_chase_without_seeing_the_player() {
        let maze = open_room(15, 15);
        let block = 20;
        // Mirando al este; el jugador queda detrás (al oeste), fuera del
        // cono de visión.
        let mut enemies = vec![Enemy::new(Vector2::new(150.0, 150.0), 'e', block as f32, 0.0)];
        let player = Player::new(Vector2::new(40.0, 150.0), 0.0, PI / 3.0);
        assert!(!can_see_player(&enemies[0], &player, &maze, block));

        alert_all(&mut enemies);
        update_enemies(&mut enemies, &player, &maze, block, 1.0);

        assert!(
            enemies[0].is_chasing(),
            "un enemigo alertado debería perseguir aunque no vea al jugador"
        );
    }

    #[test]
    fn alert_wears_off_and_enemy_goes_back_to_wandering() {
        // Dos cuartos separados por una pared: el jugador nunca queda a la
        // vista sin importar hacia dónde termine mirando el enemigo
        // mientras persigue a ciegas.
        let cells: Vec<Vec<char>> = ["+++++++++++", "+   +     +", "+   +     +", "+++++++++++"]
            .iter()
            .map(|r| r.chars().collect())
            .collect();
        let maze = Maze::new(cells);
        let block = 20;
        let mut enemies = vec![Enemy::new(Vector2::new(40.0, 30.0), 'e', block as f32, 0.0)];
        let player = Player::new(Vector2::new(140.0, 30.0), 0.0, PI / 3.0);

        alert_all(&mut enemies);
        for _ in 0..=ALERT_DURATION {
            assert!(!can_see_player(&enemies[0], &player, &maze, block));
            update_enemies(&mut enemies, &player, &maze, block, 1.0);
        }

        assert!(
            !enemies[0].is_chasing(),
            "la alerta debería haberse acabado ya"
        );
    }

    #[test]
    fn higher_speed_multiplier_covers_more_ground_while_chasing() {
        let maze = open_room(30, 5);
        let block = 20;

        let mut slow = Enemy::new(Vector2::new(40.0, 50.0), 'e', block as f32, 0.0);
        let player = Player::new(Vector2::new(400.0, 50.0), 0.0, PI / 3.0);
        chase_player(&mut slow, &player, &maze, block, 1.0);

        let mut fast = Enemy::new(Vector2::new(40.0, 50.0), 'e', block as f32, 0.0);
        chase_player(&mut fast, &player, &maze, block, 2.0);

        let slow_dist = (slow.pos() - Vector2::new(40.0, 50.0)).length();
        let fast_dist = (fast.pos() - Vector2::new(40.0, 50.0)).length();
        assert!(
            fast_dist > slow_dist,
            "un multiplicador mayor debería avanzar más en el mismo frame: lento={slow_dist} rapido={fast_dist}"
        );
    }

    #[test]
    fn footsteps_sound_more_often_at_higher_speed() {
        // Cuarto bien largo y jugador bien lejos: ni al doble de velocidad
        // el enemigo alcanza a llegar (y por lo tanto a pararse) dentro de
        // las 100 vueltas, así que la comparación no se contamina con
        // frames parado.
        let maze = open_room(60, 5);
        let block = 20;
        let player = Player::new(Vector2::new(1000.0, 50.0), 0.0, PI / 3.0);

        let count_footsteps = |speed_multiplier: f32| {
            let mut enemy = Enemy::new(Vector2::new(40.0, 50.0), 'e', block as f32, 0.0);
            let mut count = 0;
            for _ in 0..100 {
                enemy.state = State::Chasing;
                let moved = chase_player(&mut enemy, &player, &maze, block, speed_multiplier);
                assert!(moved, "el enemigo no debería haber alcanzado al jugador todavía");
                update_footstep_timer(&mut enemy, moved, speed_multiplier);
                if enemy.wants_footstep() {
                    count += 1;
                }
            }
            count
        };

        assert!(
            count_footsteps(2.0) > count_footsteps(1.0),
            "al doble de velocidad deberían sonar más pasos en el mismo número de frames"
        );
    }

    #[test]
    fn shows_front_when_facing_the_viewer() {
        let mut enemy = Enemy::new(Vector2::new(100.0, 50.0), 'e', 20.0, 0.0); // mira al este
        enemy.state = State::Idle;
        let viewer = Vector2::new(200.0, 50.0); // al este del enemigo: justo hacia donde mira

        assert_eq!(enemy.sprite_for_viewer(viewer).texture, TEX_FRONT_IDLE);
    }

    #[test]
    fn shows_back_when_facing_away_from_the_viewer() {
        let mut enemy = Enemy::new(Vector2::new(100.0, 50.0), 'e', 20.0, 0.0); // mira al este
        enemy.state = State::Idle;
        let viewer = Vector2::new(0.0, 50.0); // al oeste del enemigo: a sus espaldas

        assert_eq!(enemy.sprite_for_viewer(viewer).texture, TEX_BACK_IDLE);
    }

    #[test]
    fn chasing_alternates_run_frames_with_anim_frame() {
        let mut enemy = Enemy::new(Vector2::new(100.0, 50.0), 'e', 20.0, 0.0);
        enemy.state = State::Chasing;
        let viewer = Vector2::new(200.0, 50.0); // de frente

        enemy.anim_frame = false;
        assert_eq!(enemy.sprite_for_viewer(viewer).texture, TEX_FRONT_RUN_A);

        enemy.anim_frame = true;
        assert_eq!(enemy.sprite_for_viewer(viewer).texture, TEX_FRONT_RUN_B);
    }
}
