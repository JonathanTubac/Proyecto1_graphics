use crate::framebuffer::Framebuffer;
use crate::maze::Maze;
use crate::player::Player;
use raylib::prelude::*;

/// Resultado de lanzar un rayo: qué tan lejos quedó la pared, qué caracter
/// era y en qué punto exacto de la cara de la pared pegó (para texturizar).
pub struct Intersect {
    pub distance: f32,
    pub impact: char,
    /// Posición fraccional (0.0..1.0) del impacto a lo largo de la cara de
    /// la pared. Se escala al ancho de la textura para obtener `tx`.
    pub wall_x: f32,
    /// true si el rayo cruzó una línea horizontal de la cuadrícula (pared
    /// norte/sur), false si cruzó una vertical (pared este/oeste). Sirve
    /// para sombrear un lado distinto del otro y reforzar la profundidad.
    pub side: bool,
}

/// Avanza un rayo desde el jugador en la dirección `a` hasta topar con una
/// pared, usando DDA (Digital Differential Analysis): en vez de marchar
/// pixel por pixel, salta de línea de cuadrícula en línea de cuadrícula, lo
/// que es más rápido y da la posición exacta (no redondeada a 1px) del
/// impacto, necesaria para calcular `wall_x` sin artefactos.
pub fn cast_ray(maze: &Maze, player: &Player, a: f32, block_size: usize) -> Intersect {
    let block = block_size as f32;
    let pos_x = player.pos.x / block;
    let pos_y = player.pos.y / block;
    let ray_dir_x = a.cos();
    let ray_dir_y = a.sin();

    let mut map_x = pos_x.floor() as i32;
    let mut map_y = pos_y.floor() as i32;

    let delta_dist_x = if ray_dir_x.abs() < 1e-6 {
        f32::INFINITY
    } else {
        (1.0 / ray_dir_x).abs()
    };
    let delta_dist_y = if ray_dir_y.abs() < 1e-6 {
        f32::INFINITY
    } else {
        (1.0 / ray_dir_y).abs()
    };

    let (step_x, mut side_dist_x) = if ray_dir_x < 0.0 {
        (-1, (pos_x - map_x as f32) * delta_dist_x)
    } else {
        (1, (map_x as f32 + 1.0 - pos_x) * delta_dist_x)
    };
    let (step_y, mut side_dist_y) = if ray_dir_y < 0.0 {
        (-1, (pos_y - map_y as f32) * delta_dist_y)
    } else {
        (1, (map_y as f32 + 1.0 - pos_y) * delta_dist_y)
    };

    let mut side;
    loop {
        if side_dist_x < side_dist_y {
            side_dist_x += delta_dist_x;
            map_x += step_x;
            side = false;
        } else {
            side_dist_y += delta_dist_y;
            map_y += step_y;
            side = true;
        }

        if map_x < 0 || map_y < 0 || map_x as usize >= maze.width() || map_y as usize >= maze.height() {
            // El rayo se salió del mapa (no debería pasar con un laberinto
            // bien cerrado, pero evita un ciclo infinito si algún día no lo está).
            let perp = if side { side_dist_y - delta_dist_y } else { side_dist_x - delta_dist_x };
            return Intersect {
                distance: perp.max(0.0) * block,
                impact: ' ',
                wall_x: 0.0,
                side,
            };
        }

        if maze.is_wall(map_x as usize, map_y as usize) {
            break;
        }
    }

    let perp_wall_dist = if side {
        side_dist_y - delta_dist_y
    } else {
        side_dist_x - delta_dist_x
    };

    let wall_x = if side {
        pos_x + perp_wall_dist * ray_dir_x
    } else {
        pos_y + perp_wall_dist * ray_dir_y
    };
    let mut wall_x = wall_x - wall_x.floor();

    // Evita que la textura salga espejeada: sin esto, la mitad de las caras
    // (según de qué lado se golpeen) mostrarían la imagen al revés.
    if !side && ray_dir_x > 0.0 {
        wall_x = 1.0 - wall_x;
    }
    if side && ray_dir_y < 0.0 {
        wall_x = 1.0 - wall_x;
    }

    Intersect {
        distance: perp_wall_dist * block,
        impact: maze.get(map_x as usize, map_y as usize),
        wall_x,
        side,
    }
}

/// Dibuja el segmento recorrido por un rayo, sólo para depuración visual en
/// el mapa 2D; no participa en el cálculo del impacto.
pub fn draw_ray_path(framebuffer: &mut Framebuffer, player: &Player, a: f32, distance: f32) {
    framebuffer.set_current_color(Color::new(230, 230, 240, 255));
    let (cos, sin) = (a.cos(), a.sin());
    let mut d = 0.0;
    while d < distance {
        let x = player.pos.x + d * cos;
        let y = player.pos.y + d * sin;
        framebuffer.set_pixel(x as i32, y as i32);
        d += 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maze::Maze;

    #[test]
    fn ray_hits_wall_at_expected_distance() {
        let cells: Vec<Vec<char>> = ["+--+", "|  |", "+--+"]
            .iter()
            .map(|r| r.chars().collect())
            .collect();
        let maze = Maze::new(cells);
        // Centro de la celda (1,1) con bloques de 10 px.
        let player = Player::at_cell(1, 1, 10, 0.0, 0.0);

        let hit = cast_ray(&maze, &player, 0.0, 10);
        assert_eq!(hit.impact, '|');
        assert!((hit.distance - 15.0).abs() < 1.5, "d = {}", hit.distance);

        // Hacia arriba (-PI/2) topa con la pared horizontal de arriba.
        let hit = cast_ray(&maze, &player, -std::f32::consts::PI / 2.0, 10);
        assert_eq!(hit.impact, '-');
        assert!((hit.distance - 5.0).abs() < 1.5, "d = {}", hit.distance);
    }

    #[test]
    fn wall_x_stays_within_unit_range() {
        let cells: Vec<Vec<char>> = ["+--+", "|  |", "+--+"]
            .iter()
            .map(|r| r.chars().collect())
            .collect();
        let maze = Maze::new(cells);
        let player = Player::at_cell(1, 1, 10, 0.0, 0.0);

        for i in 0..36 {
            let a = i as f32 * (std::f32::consts::TAU / 36.0);
            let hit = cast_ray(&maze, &player, a, 10);
            assert!((0.0..=1.0).contains(&hit.wall_x), "wall_x = {}", hit.wall_x);
        }
    }
}
