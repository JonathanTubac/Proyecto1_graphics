use crate::maze::Maze;
use crate::player::Player;
use crate::sprites::Sprite;
use raylib::prelude::*;

/// Qué tan cerca hay que estar de un tótem para poder destruirlo.
const INTERACT_RANGE: f32 = 40.0;

/// Un ídolo fijo que hay que destruir para poder abrir la puerta de salida.
/// No tiene IA (no se mueve, no ve al jugador): sólo guarda si ya lo
/// destruyeron o no.
pub struct Totem {
    pos: Vector2,
    size: f32,
    destroyed: bool,
}

impl Totem {
    #[allow(dead_code)]
    pub fn pos(&self) -> Vector2 {
        self.pos
    }

    #[allow(dead_code)]
    pub fn is_destroyed(&self) -> bool {
        self.destroyed
    }

    /// Sprite a dibujar, o `None` si ya se destruyó (no queda nada que
    /// mostrar).
    pub fn sprite(&self) -> Option<Sprite> {
        if self.destroyed {
            None
        } else {
            Some(Sprite::new(self.pos, 't', self.size))
        }
    }
}

/// Busca en el laberinto las celdas marcadas con `marker` y planta un tótem
/// en cada una.
pub fn spawn_from_maze(maze: &Maze, block_size: usize, marker: char) -> Vec<Totem> {
    let half = block_size as f32 / 2.0;
    maze.find_all(marker)
        .into_iter()
        .map(|(x, y)| Totem {
            pos: Vector2::new(
                x as f32 * block_size as f32 + half,
                y as f32 * block_size as f32 + half,
            ),
            size: block_size as f32,
            destroyed: false,
        })
        .collect()
}

/// Índice del tótem sin destruir más cercano, si el jugador está lo
/// bastante cerca de alguno como para destruirlo. No lo destruye: sólo
/// avisa que se puede, para que quien llama decida según el input (y para
/// poder mostrar una pista en pantalla aunque no se presione la tecla).
pub fn interactable(totems: &[Totem], player: &Player) -> Option<usize> {
    totems
        .iter()
        .position(|t| !t.destroyed && (t.pos - player.pos).length() < INTERACT_RANGE)
}

/// Destruye el tótem en `index`, si existe y no estaba ya destruido.
pub fn destroy(totems: &mut [Totem], index: usize) {
    if let Some(t) = totems.get_mut(index) {
        t.destroyed = true;
    }
}

pub fn remaining(totems: &[Totem]) -> usize {
    totems.iter().filter(|t| !t.destroyed).count()
}

pub fn all_destroyed(totems: &[Totem]) -> bool {
    remaining(totems) == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn make_totem(x: f32, y: f32) -> Totem {
        Totem {
            pos: Vector2::new(x, y),
            size: 40.0,
            destroyed: false,
        }
    }

    #[test]
    fn interactable_only_within_range() {
        let totems = vec![make_totem(0.0, 0.0)];
        let near = Player::new(Vector2::new(20.0, 0.0), 0.0, PI / 3.0);
        let far = Player::new(Vector2::new(500.0, 0.0), 0.0, PI / 3.0);

        assert_eq!(interactable(&totems, &near), Some(0));
        assert_eq!(interactable(&totems, &far), None);
    }

    #[test]
    fn destroyed_totem_is_not_interactable_or_drawn() {
        let mut totems = vec![make_totem(0.0, 0.0)];
        let player = Player::new(Vector2::new(20.0, 0.0), 0.0, PI / 3.0);

        destroy(&mut totems, 0);

        assert_eq!(interactable(&totems, &player), None);
        assert!(totems[0].sprite().is_none());
    }

    #[test]
    fn all_destroyed_tracks_remaining_count() {
        let mut totems = vec![make_totem(0.0, 0.0), make_totem(100.0, 100.0)];
        assert_eq!(remaining(&totems), 2);
        assert!(!all_destroyed(&totems));

        destroy(&mut totems, 0);
        assert_eq!(remaining(&totems), 1);
        assert!(!all_destroyed(&totems));

        destroy(&mut totems, 1);
        assert_eq!(remaining(&totems), 0);
        assert!(all_destroyed(&totems));
    }
}
