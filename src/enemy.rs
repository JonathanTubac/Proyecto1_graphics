use crate::maze::Maze;
use crate::player::Player;
use crate::sprites::Sprite;
use raylib::prelude::*;
use std::f32::consts::PI;

/// Qué tan lejos puede ver un enemigo antes de perder al jugador de vista.
const SIGHT_RANGE: f32 = 220.0;
/// Ancho total del cono de visión (a cada lado del "facing" hay la mitad).
/// ~103°: bastante amplio para que un guardia se sienta atento, sin ser
/// omnisciente.
const SIGHT_FOV: f32 = 1.8;
/// Pixeles por frame que avanza un enemigo mientras persigue.
const CHASE_SPEED: f32 = 1.8;
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
    /// actualiza para apuntar siempre al jugador.
    facing: f32,
    state: State,
}

impl Enemy {
    fn new(pos: Vector2, texture: char, size: f32, facing: f32) -> Self {
        Enemy {
            sprite: Sprite::new(pos, texture, size),
            facing,
            state: State::Idle,
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
    maze.find_all(marker)
        .into_iter()
        .map(|(x, y)| {
            let pos = Vector2::new(
                x as f32 * block_size as f32 + half,
                y as f32 * block_size as f32 + half,
            );
            Enemy::new(pos, texture, block_size as f32, initial_facing)
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
/// este frame.
pub fn update_enemies(enemies: &mut [Enemy], player: &Player, maze: &Maze, block_size: usize) {
    for enemy in enemies {
        enemy.state = if can_see_player(enemy, player, maze, block_size) {
            State::Chasing
        } else {
            State::Idle
        };

        if enemy.state != State::Chasing {
            continue;
        }

        let dx = player.pos.x - enemy.pos().x;
        let dy = player.pos.y - enemy.pos().y;
        let distance = (dx * dx + dy * dy).sqrt();

        // Sigue mirando hacia el jugador aunque ya no avance más: es lo que
        // hace que el cono de visión "lo siga teniendo encima" mientras esté
        // cerca, en vez de perderlo de vista por quedarse mirando fijo.
        enemy.facing = dy.atan2(dx);

        if distance < STOP_DISTANCE {
            continue; // ya está lo bastante cerca, no hace falta seguir avanzando
        }

        let (step_x, step_y) = (dx / distance * CHASE_SPEED, dy / distance * CHASE_SPEED);
        let pos = &mut enemy.sprite.pos;

        // Cada eje se prueba por separado: si uno choca, el otro todavía
        // puede avanzar y el enemigo se desliza sobre la pared en vez de
        // trabarse en una esquina.
        if maze.is_free(pos.x + step_x, pos.y, block_size, RADIUS) {
            pos.x += step_x;
        }
        if maze.is_free(pos.x, pos.y + step_y, block_size, RADIUS) {
            pos.y += step_y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
