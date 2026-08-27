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

    pub fn is_wall(&self, x: usize, y: usize) -> bool {
        matches!(self.get(x, y), '+' | '-' | '|')
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
