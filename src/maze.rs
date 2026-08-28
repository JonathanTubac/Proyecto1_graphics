use std::fs::File;
use std::io::{BufRead, BufReader};

/// Un laberinto es una cuadrícula de caracteres:
///   '+', '-', '|'  paredes
///   ' '            piso
///   'p'            posición inicial del jugador
///   'g'            meta
pub struct Maze {
    cells: Vec<Vec<char>>,
}

impl Maze {
    pub fn new(cells: Vec<Vec<char>>) -> Self {
        Maze { cells }
    }

    /// Cantidad de filas.
    pub fn height(&self) -> usize {
        self.cells.len()
    }

    /// Cantidad de columnas de la fila más larga: el archivo puede traer
    /// líneas cortas porque los espacios finales suelen recortarse.
    pub fn width(&self) -> usize {
        self.cells.iter().map(|row| row.len()).max().unwrap_or(0)
    }

    /// Caracter en (x, y). Fuera del mapa o de una fila corta se trata como piso.
    pub fn get(&self, x: usize, y: usize) -> char {
        self.cells
            .get(y)
            .and_then(|row| row.get(x))
            .copied()
            .unwrap_or(' ')
    }

    /// Los lockers ('l') cuentan como pared: son un mueble sólido empotrado
    /// en el muro (ver `crate::locker`), no algo que se camine encima. El
    /// jugador se para junto a la celda, no sobre ella.
    pub fn is_wall(&self, x: usize, y: usize) -> bool {
        matches!(self.get(x, y), '+' | '-' | '|' | 'l')
    }

    /// Primera celda que contiene `target`, en coordenadas (x, y).
    pub fn find(&self, target: char) -> Option<(usize, usize)> {
        self.find_all(target).into_iter().next()
    }

    /// Todas las celdas que contienen `target`, en coordenadas (x, y).
    /// Se usa para ubicar spawns de sprites (enemigos, items) marcados en
    /// el propio archivo del laberinto.
    pub fn find_all(&self, target: char) -> Vec<(usize, usize)> {
        let mut found = Vec::new();
        for (y, row) in self.cells.iter().enumerate() {
            for (x, &cell) in row.iter().enumerate() {
                if cell == target {
                    found.push((x, y));
                }
            }
        }
        found
    }

    pub fn player_start(&self) -> Option<(usize, usize)> {
        self.find('p')
    }

    pub fn goal(&self) -> Option<(usize, usize)> {
        self.find('g')
    }

    /// ¿Cabe un punto (jugador o enemigo) con centro en (x, y), en pixeles
    /// del mundo, y radio de colisión `radius`? Revisa las cuatro esquinas
    /// de su caja, así no se mete de lado a una pared. La comparten el
    /// jugador y los enemigos para no duplicar la misma lógica.
    pub fn is_free(&self, x: f32, y: f32, block_size: usize, radius: f32) -> bool {
        for (ox, oy) in [
            (-radius, -radius),
            (radius, -radius),
            (-radius, radius),
            (radius, radius),
        ] {
            let px = x + ox;
            let py = y + oy;
            if px < 0.0 || py < 0.0 {
                return false;
            }
            if self.is_wall(px as usize / block_size, py as usize / block_size) {
                return false;
            }
        }
        true
    }

    /// ¿Hay línea recta libre de paredes entre dos puntos del mundo (en
    /// pixeles)? Camina la línea a pasos fijos y revisa si algún punto cae
    /// en una celda de pared. No necesita la precisión de un DDA porque es
    /// una consulta de sí/no para IA (visión de un enemigo), no un render.
    pub fn line_of_sight(&self, from: (f32, f32), to: (f32, f32), block_size: usize) -> bool {
        const STEP: f32 = 4.0;

        let dx = to.0 - from.0;
        let dy = to.1 - from.1;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance < 1.0 {
            return true;
        }
        let (step_x, step_y) = (dx / distance * STEP, dy / distance * STEP);

        let (mut x, mut y) = from;
        let mut walked = 0.0;
        while walked < distance {
            if x < 0.0 || y < 0.0 {
                return false;
            }
            if self.is_wall(x as usize / block_size, y as usize / block_size) {
                return false;
            }
            x += step_x;
            y += step_y;
            walked += STEP;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall_between_two_rooms() -> Maze {
        // "+-----+"
        // "|  |  |"
        // "+-----+"
        let cells: Vec<Vec<char>> = ["+-----+", "|  |  |", "+-----+"]
            .iter()
            .map(|r| r.chars().collect())
            .collect();
        Maze::new(cells)
    }

    #[test]
    fn line_of_sight_clear_within_same_open_room() {
        let maze = wall_between_two_rooms();
        // Ambos puntos caen en la celda (1,1), sin pared de por medio.
        assert!(maze.line_of_sight((12.0, 15.0), (18.0, 15.0), 10));
    }

    #[test]
    fn line_of_sight_blocked_by_wall_between_rooms() {
        let maze = wall_between_two_rooms();
        // (1,1) y (4,1) están en cuartos distintos separados por la pared
        // en la celda (3,1).
        assert!(!maze.line_of_sight((15.0, 15.0), (45.0, 15.0), 10));
    }

    #[test]
    fn is_free_rejects_points_overlapping_a_wall() {
        let maze = wall_between_two_rooms();
        assert!(maze.is_free(15.0, 15.0, 10, 4.0));
        // Con radio 4, un centro a 2px de la pared en x=30 ya la toca.
        assert!(!maze.is_free(28.0, 15.0, 10, 4.0));
    }
}

pub fn load_maze(filename: &str) -> Maze {
    let file = File::open(filename)
        .unwrap_or_else(|e| panic!("No se pudo abrir el laberinto '{filename}': {e}"));
    let reader = BufReader::new(file);

    let cells = reader
        .lines()
        .map(|line| line.unwrap().chars().collect())
        .collect();

    Maze::new(cells)
}
