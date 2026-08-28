use crate::maze::Maze;
use crate::player::Player;
use raylib::prelude::*;

/// Qué tan cerca hay que estar de un locker para poder entrar o salir de él.
const INTERACT_RANGE: f32 = 40.0;

/// Un mueble empotrado en la pared en el que el jugador puede esconderse del
/// enemigo. Su celda cuenta como pared (`Maze::is_wall`), así que se dibuja
/// y colisiona como cualquier otro muro (ver `crate::caster`, que ya
/// texturiza con el caracter que haya en la celda) y no necesita su propio
/// sprite billboard. A diferencia de un `Totem`, no tiene estado propio
/// (nunca se "gasta"): quién está escondido y dónde es responsabilidad de
/// `Player::hidden`, no de esta estructura. Ver cómo lo respetan
/// `enemy::can_see_player` y `enemy::damage_player_if_close`.
pub struct Locker {
    pos: Vector2,
}

/// Busca en el laberinto las celdas marcadas con `marker` y planta un
/// locker en cada una.
pub fn spawn_from_maze(maze: &Maze, block_size: usize, marker: char) -> Vec<Locker> {
    let half = block_size as f32 / 2.0;
    maze.find_all(marker)
        .into_iter()
        .map(|(x, y)| Locker {
            pos: Vector2::new(
                x as f32 * block_size as f32 + half,
                y as f32 * block_size as f32 + half,
            ),
        })
        .collect()
}

/// ¿Hay algún locker lo bastante cerca del jugador como para entrar o salir
/// de él? No hace falta saber cuál (a diferencia de un tótem, ninguno tiene
/// estado propio que cambiar), así que basta con un booleano.
pub fn nearby(lockers: &[Locker], player: &Player) -> bool {
    lockers.iter().any(|l| (l.pos - player.pos).length() < INTERACT_RANGE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn make_locker(x: f32, y: f32) -> Locker {
        Locker { pos: Vector2::new(x, y) }
    }

    #[test]
    fn nearby_only_within_range() {
        let lockers = vec![make_locker(0.0, 0.0)];
        let near = Player::new(Vector2::new(20.0, 0.0), 0.0, PI / 3.0);
        let far = Player::new(Vector2::new(500.0, 0.0), 0.0, PI / 3.0);

        assert!(nearby(&lockers, &near));
        assert!(!nearby(&lockers, &far));
    }

    #[test]
    fn nearby_is_false_with_no_lockers() {
        let player = Player::new(Vector2::new(0.0, 0.0), 0.0, PI / 3.0);
        assert!(!nearby(&[], &player));
    }
}
