use crate::maze::Maze;
use raylib::prelude::*;
use std::collections::{HashMap, VecDeque};

/// Coordenada de una celda del laberinto (columna, fila), no pixeles.
pub type Cell = (usize, usize);

/// A qué celda de la grilla cae una posición del mundo.
pub fn to_cell(pos: Vector2, block_size: usize) -> Cell {
    (
        (pos.x / block_size as f32).max(0.0) as usize,
        (pos.y / block_size as f32).max(0.0) as usize,
    )
}

/// Centro, en pixeles del mundo, de una celda de la grilla.
pub fn cell_center(cell: Cell, block_size: usize) -> Vector2 {
    let half = block_size as f32 / 2.0;
    Vector2::new(
        cell.0 as f32 * block_size as f32 + half,
        cell.1 as f32 * block_size as f32 + half,
    )
}

/// Celdas transitables (arriba/abajo/izquierda/derecha) alrededor de `cell`.
/// Pública para que `crate::enemy` elija a dónde patrullar con el mismo
/// criterio de "libre" que usa el propio pathfinding.
pub fn free_neighbors(maze: &Maze, cell: Cell) -> Vec<Cell> {
    let (x, y) = cell;
    let width = maze.width();
    let height = maze.height();
    let mut result = Vec::with_capacity(4);

    let consider = |nx: usize, ny: usize, result: &mut Vec<Cell>| {
        if !maze.is_wall(nx, ny) {
            result.push((nx, ny));
        }
    };

    if x + 1 < width {
        consider(x + 1, y, &mut result);
    }
    if x > 0 {
        consider(x - 1, y, &mut result);
    }
    if y + 1 < height {
        consider(x, y + 1, &mut result);
    }
    if y > 0 {
        consider(x, y - 1, &mut result);
    }

    result
}

/// Ruta más corta entre dos celdas transitables, en cantidad de pasos
/// (sin contar la celda de salida). Como todas las celdas cuestan lo mismo,
/// un BFS visita en el mismo orden que un Dijkstra pero sin necesitar la
/// cola de prioridad que ese algoritmo trae para pesos distintos: acá no
/// hace falta. `None` si no hay camino (laberinto desconectado, o alguna de
/// las dos celdas es pared).
pub fn find_path(maze: &Maze, start: Cell, goal: Cell) -> Option<Vec<Cell>> {
    if maze.is_wall(start.0, start.1) || maze.is_wall(goal.0, goal.1) {
        return None;
    }
    if start == goal {
        return Some(Vec::new());
    }

    let mut came_from: HashMap<Cell, Cell> = HashMap::new();
    came_from.insert(start, start);
    let mut queue = VecDeque::new();
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        if current == goal {
            return Some(reconstruct(&came_from, start, goal));
        }
        for next in free_neighbors(maze, current) {
            if !came_from.contains_key(&next) {
                came_from.insert(next, current);
                queue.push_back(next);
            }
        }
    }

    None
}

/// Reconstruye la ruta caminando `came_from` desde `goal` hasta `start`, sin
/// incluir `start` (el que la sigue ya está parado ahí).
fn reconstruct(came_from: &HashMap<Cell, Cell>, start: Cell, goal: Cell) -> Vec<Cell> {
    let mut path = vec![goal];
    let mut current = goal;
    while current != start {
        current = came_from[&current];
        path.push(current);
    }
    path.reverse();
    path.remove(0);
    path
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn finds_a_straight_path_in_an_open_room() {
        let maze = open_room(10, 5);
        let path = find_path(&maze, (1, 1), (7, 1)).expect("debería encontrar camino");
        assert_eq!(path.last(), Some(&(7, 1)));
        assert_eq!(path.len(), 6); // 6 pasos de (1,1) a (7,1)
    }

    #[test]
    fn routes_around_a_wall_between_two_rooms() {
        // Dos cuartos (cols 1-3 y 5-9) separados por una pared en x=4, con
        // un hueco en la fila 2 que los conecta.
        let cells: Vec<Vec<char>> = [
            "+++++++++++",
            "+   +     +",
            "+        +",
            "+   +     +",
            "+++++++++++",
        ]
        .iter()
        .map(|r| r.chars().collect())
        .collect();
        let maze = Maze::new(cells);

        let path = find_path(&maze, (1, 1), (9, 1)).expect("debería rodear la pared");
        assert!(
            path.iter().all(|&(x, y)| !maze.is_wall(x, y)),
            "la ruta no debería pasar por ninguna celda de pared"
        );
        assert_eq!(path.last(), Some(&(9, 1)));
    }

    #[test]
    fn no_path_between_disconnected_rooms() {
        let cells: Vec<Vec<char>> = ["+++++++++++", "+   +     +", "+   +     +", "+++++++++++"]
            .iter()
            .map(|r| r.chars().collect())
            .collect();
        let maze = Maze::new(cells);

        assert!(find_path(&maze, (1, 1), (9, 1)).is_none());
    }

    #[test]
    fn same_cell_returns_an_empty_path() {
        let maze = open_room(5, 5);
        assert_eq!(find_path(&maze, (2, 2), (2, 2)), Some(Vec::new()));
    }
}
