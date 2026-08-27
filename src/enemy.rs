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
/// Radio de colisión del enemigo contra las paredes (mismo criterio que el jugador).
const RADIUS: f32 = 8.0;

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
}

impl Enemy {
    fn new(pos: Vector2, texture: char, size: f32, facing: f32) -> Self {
        Enemy {
            sprite: Sprite::new(pos, texture, size),
            facing,
            state: State::Idle,
            wander_dir: facing,
            wander_timer: 0,
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
/// medio.
fn can_see_player(enemy: &Enemy, player: &Player, maze: &Maze, block_size: usize) -> bool {
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

/// Actualiza la visión, el estado y la posición de todos los enemigos para
/// este frame: persiguen si ven al jugador, patrullan solos si no.
pub fn update_enemies(enemies: &mut [Enemy], player: &Player, maze: &Maze, block_size: usize) {
    let mut rng = rand::thread_rng();
    for enemy in enemies {
        enemy.state = if can_see_player(enemy, player, maze, block_size) {
            State::Chasing
        } else {
            State::Idle
        };

        match enemy.state {
            State::Chasing => chase_player(enemy, player, maze, block_size),
            State::Idle => wander(enemy, maze, block_size, &mut rng),
        }
    }
}

/// Camina en línea recta hacia el jugador, deslizándose sobre las paredes
/// igual que el jugador, y se para a `STOP_DISTANCE` para no superponerse
/// con la cámara.
fn chase_player(enemy: &mut Enemy, player: &Player, maze: &Maze, block_size: usize) {
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
        return; // ya está lo bastante cerca, no hace falta seguir avanzando
    }

    let (step_x, step_y) = (dx / distance * CHASE_SPEED, dy / distance * CHASE_SPEED);
    move_with_wall_slide(enemy, maze, block_size, step_x, step_y);
}

/// Camina solo en una dirección hasta chocar con una pared o hasta que se
/// cumpla el tiempo de ronda, momento en el que elige una dirección nueva al
/// azar. Es un recorrido simple ("rebota" en las paredes), no un patrullaje
/// por puntos fijos, pero alcanza para que el laberinto se sienta habitado.
fn wander(enemy: &mut Enemy, maze: &Maze, block_size: usize, rng: &mut impl Rng) {
    if enemy.wander_timer == 0 {
        enemy.wander_dir = rng.gen_range(0.0..TAU);
        enemy.wander_timer = WANDER_CHANGE_INTERVAL;
    } else {
        enemy.wander_timer -= 1;
    }
    enemy.facing = enemy.wander_dir;

    let step_x = enemy.wander_dir.cos() * WANDER_SPEED;
    let step_y = enemy.wander_dir.sin() * WANDER_SPEED;
    let moved = move_with_wall_slide(enemy, maze, block_size, step_x, step_y);

    // Chocó de lleno (no avanzó en ningún eje): no tiene caso esperar al
    // timer, mejor elegir otra dirección ya para no quedarse vibrando
    // contra la pared.
    if !moved {
        enemy.wander_timer = 0;
    }
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
            wander(&mut enemy, &maze, block, &mut rng);
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
            wander(&mut enemy, &maze, block, &mut rng);
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
        wander(&mut enemy, &maze, block, &mut rng);

        assert_eq!(enemy.wander_timer, 0, "debería forzar un cambio de dirección al chocar");
    }
}
