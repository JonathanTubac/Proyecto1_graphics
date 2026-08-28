use crate::maze::Maze;
use crate::pathfind::{self, Cell};
use crate::player::Player;
use crate::sprites::Sprite;
use rand::Rng;
use raylib::prelude::*;
use std::f32::consts::PI;

/// Qué tan lejos puede ver un enemigo antes de perder al jugador de vista.
const SIGHT_RANGE: f32 = 220.0;
/// Ancho total del cono de visión (a cada lado del "facing" hay la mitad).
/// ~103°: bastante amplio para que un guardia se sienta atento, sin ser
/// omnisciente.
const SIGHT_FOV: f32 = 1.8;
/// Pixeles por frame que avanza un enemigo mientras persigue o investiga.
const CHASE_SPEED: f32 = 1.8;
/// Pixeles por frame que camina un enemigo mientras patrulla solo. Más
/// lento que perseguir, para que se note la diferencia entre "va de ronda"
/// y "ya me vio" (o "cree saber dónde está").
const WANDER_SPEED: f32 = 0.9;
/// No se acerca más que esto al jugador, para no superponer su sprite con
/// la cámara, ni a la celda que está investigando (ahí ya no hay nadie que
/// alcanzar).
const STOP_DISTANCE: f32 = 24.0;
/// Qué tan cerca tiene que estar un enemigo para hacerle daño al jugador. Un
/// poco más que `STOP_DISTANCE`, así que en la práctica pasa en cuanto un
/// enemigo que persigue alcanza al jugador y se para.
const DAMAGE_RANGE: f32 = 26.0;
/// Radio de colisión del enemigo contra las paredes (mismo criterio que el jugador).
const RADIUS: f32 = 8.0;
/// Qué tan cerca de un punto de la ruta hay que estar para darlo por
/// alcanzado y pasar al siguiente. Más chico que un paso normal, para que no
/// se "corten" esquinas de forma notoria.
const WAYPOINT_TOLERANCE: f32 = 4.0;
/// Cada cuántos frames suena un paso del enemigo mientras se mueve, a
/// velocidad base (a 60 fps, ~0.35s entre pasos). Se acorta según qué tan
/// rápido esté yendo en este momento.
const BASE_FOOTSTEP_INTERVAL: f32 = 21.0;

/// Caracteres de textura para cada combinación de vista (de frente / de
/// espaldas) y pose (quieto, o cuadro A/B de la animación de correr).
const TEX_FRONT_IDLE: char = 'e';
const TEX_BACK_IDLE: char = 'b';
const TEX_FRONT_RUN_A: char = '1';
const TEX_FRONT_RUN_B: char = '2';
const TEX_BACK_RUN_A: char = '3';
const TEX_BACK_RUN_B: char = '4';

/// Qué está haciendo el enemigo ahora mismo:
/// - `Idle`: patrulla solo, sin pista de dónde está el jugador.
/// - `Investigating`: no lo ve, pero va derecho hacia la última celda donde
///   se supo de él (lo vio ahí, o ahí fue el estruendo de un tótem
///   rompiéndose); si llega y no hay nadie, pierde el rastro y vuelve a
///   `Idle`.
/// - `Chasing`: lo tiene a la vista ahora mismo.
#[derive(Clone, Copy, PartialEq)]
enum State {
    Idle,
    Investigating,
    Chasing,
}

/// Un enemigo: dónde está, hacia dónde "mira" (su cono de visión) y qué tan
/// al tanto está del jugador. Produce un `Sprite` para que `crate::sprites`
/// lo dibuje; no sabe nada de texturizado ni de billboards.
pub struct Enemy {
    pub sprite: Sprite,
    /// Dirección en la que "vigila", en radianes. Persiguiendo, apunta al
    /// jugador de verdad (para que el cono de visión lo siga teniendo
    /// encima); investigando o patrullando, apunta hacia donde caminan los
    /// pies (el siguiente punto de la ruta).
    facing: f32,
    state: State,
    /// Celda de la que salió el último paso de patrulla, para no dar
    /// media vuelta de inmediato y patrullar de forma más natural (salvo en
    /// un callejón sin salida, donde no queda de otra).
    last_wander_cell: Option<Cell>,
    /// Última celda donde se supo del jugador (lo vio ahí, o fue la alerta
    /// de un tótem roto), mientras `state == Investigating`. `None` en
    /// cuanto llega y no encuentra a nadie: ahí es cuando "pierde el
    /// rastro".
    investigate_target: Option<Cell>,
    /// Ruta pendiente hacia el objetivo actual (el jugador mientras
    /// persigue, `investigate_target` mientras investiga, o la siguiente
    /// celda de ronda mientras patrulla), en centros de celda ya
    /// convertidos a pixeles. Se recalcula con `crate::pathfind` cuando la
    /// celda objetivo cambia o se vacía.
    path: Vec<Vector2>,
    /// Celda objetivo con la que se calculó `path` la última vez, para
    /// saber si hace falta recalcularla (el jugador se movió a otra celda)
    /// en vez de rehacerla cada frame sin necesidad.
    path_target: Option<Cell>,
    /// Frames que faltan para el siguiente sonido de paso.
    footstep_timer: u32,
    /// Si toca reproducir un sonido de paso justo este frame.
    footstep_due: bool,
    /// Cuadro de la animación de correr (alterna en cada paso; ver
    /// `sprite_for_viewer`).
    anim_frame: bool,
}

impl Enemy {
    fn new(pos: Vector2, texture: char, size: f32, facing: f32) -> Self {
        Enemy {
            sprite: Sprite::new(pos, texture, size),
            facing,
            state: State::Idle,
            last_wander_cell: None,
            investigate_target: None,
            path: Vec::new(),
            path_target: None,
            footstep_timer: 0,
            footstep_due: false,
            anim_frame: false,
        }
    }

    pub fn pos(&self) -> Vector2 {
        self.sprite.pos
    }

    /// Si está persiguiendo o investigando al jugador ahora mismo (por si
    /// más adelante se quiere, por ejemplo, cambiarle el color o la
    /// textura).
    #[allow(dead_code)]
    pub fn is_hunting(&self) -> bool {
        self.state != State::Idle
    }

    /// Si lo tiene a la vista ahora mismo (a diferencia de ir hacia una
    /// última posición conocida sin verlo).
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
    /// queda `viewer` (el jugador) respecto a él; mientras persigue o
    /// investiga, además alterna dos cuadros de carrera al ritmo de sus
    /// propios pasos (el mismo `footstep_timer` que dispara el sonido).
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
            (State::Chasing, true, false) | (State::Investigating, true, false) => TEX_FRONT_RUN_A,
            (State::Chasing, true, true) | (State::Investigating, true, true) => TEX_FRONT_RUN_B,
            (State::Chasing, false, false) | (State::Investigating, false, false) => TEX_BACK_RUN_A,
            (State::Chasing, false, true) | (State::Investigating, false, true) => TEX_BACK_RUN_B,
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
/// este frame.
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
        let seen = can_see_player(enemy, player, maze, block_size);

        if seen {
            enemy.state = State::Chasing;
        } else if enemy.state == State::Chasing {
            // Lo tenía a la vista hasta este mismo frame: ahora va a
            // investigar el último lugar donde lo vio, en vez de saber
            // mágicamente hacia dónde se fue.
            enemy.state = State::Investigating;
            enemy.investigate_target = Some(pathfind::to_cell(player.pos, block_size));
            enemy.path.clear();
            enemy.path_target = None;
        }

        let moved = match enemy.state {
            State::Chasing => chase_player(enemy, player, maze, block_size, speed_multiplier),
            State::Investigating => investigate(enemy, maze, block_size, speed_multiplier),
            State::Idle => wander(enemy, maze, block_size, speed_multiplier, &mut rng),
        };

        update_footstep_timer(enemy, moved, speed_multiplier);
    }
}

/// Manda a todos los enemigos a investigar la posición actual del jugador,
/// como si hubieran oído el estruendo de un tótem rompiéndose: no lo ven
/// (si lo vieran, ya estarían persiguiendo de verdad), pero van derecho
/// hacia ahí. Se llama cada vez que se destruye un tótem, así que romper
/// varios seguidos refresca el objetivo en vez de dejar que el enemigo siga
/// una pista ya vieja.
pub fn alert_all(enemies: &mut [Enemy], player_pos: Vector2, block_size: usize) {
    let target = pathfind::to_cell(player_pos, block_size);
    for enemy in enemies {
        if enemy.state == State::Chasing {
            // Ya lo tiene a la vista: nada que avisarle.
            continue;
        }
        enemy.state = State::Investigating;
        enemy.investigate_target = Some(target);
        enemy.path.clear();
        enemy.path_target = None;
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

/// Si la ruta hacia `target_cell` ya no sirve (está vacía, o se calculó para
/// otra celda objetivo), la recalcula con `crate::pathfind` y la deja lista
/// en `enemy.path`. `Vec::new()` (ruta vacía porque no hay camino, o porque
/// ya se está en `target_cell`) es una respuesta válida: quien llama decide
/// qué hacer si no hay a dónde ir.
fn ensure_path_towards(enemy: &mut Enemy, maze: &Maze, block_size: usize, target_cell: Cell) {
    if !enemy.path.is_empty() && enemy.path_target == Some(target_cell) {
        return;
    }

    let start_cell = pathfind::to_cell(enemy.pos(), block_size);
    enemy.path = pathfind::find_path(maze, start_cell, target_cell)
        .map(|cells| {
            cells
                .into_iter()
                .map(|c| pathfind::cell_center(c, block_size))
                .collect()
        })
        .unwrap_or_default();
    enemy.path_target = Some(target_cell);
}

/// Avanza hacia el jugador siguiendo la ruta que calcula `crate::pathfind`
/// (así no se traba en las esquinas del laberinto), pero sin dejar de
/// mirarlo de frente: los ojos se quedan fijos en su posición real, aunque
/// los pies vayan por el camino que rodea las paredes. Se para a
/// `STOP_DISTANCE` para no superponerse con la cámara. Regresa si avanzó de
/// verdad este frame.
fn chase_player(
    enemy: &mut Enemy,
    player: &Player,
    maze: &Maze,
    block_size: usize,
    speed_multiplier: f32,
) -> bool {
    let to_player = player.pos - enemy.pos();
    let distance = to_player.length();
    let look_at_player = to_player.y.atan2(to_player.x);

    if distance < STOP_DISTANCE {
        enemy.facing = look_at_player;
        enemy.path.clear();
        enemy.path_target = None;
        return false;
    }

    let target_cell = pathfind::to_cell(player.pos, block_size);
    ensure_path_towards(enemy, maze, block_size, target_cell);

    let speed = CHASE_SPEED * speed_multiplier;
    let moved = if enemy.path.is_empty() {
        // Sin ruta (misma celda que el jugador, o el pathfinding no
        // encontró camino, lo que no debería pasar en un laberinto
        // conexo): igual avanza en línea recta para no quedarse pegado.
        let (step_x, step_y) = (to_player.x / distance * speed, to_player.y / distance * speed);
        move_with_wall_slide(enemy, maze, block_size, step_x, step_y)
    } else {
        follow_path(enemy, maze, block_size, speed)
    };

    // Los ojos se quedan fijos en el jugador (para que el cono de visión lo
    // siga teniendo encima) aunque `follow_path` haya reorientado los pies
    // hacia el siguiente punto de la ruta.
    enemy.facing = look_at_player;
    moved
}

/// Avanza hacia `investigate_target` (la última celda donde se supo del
/// jugador) siguiendo una ruta calculada, igual que `chase_player` pero sin
/// nadie a quien mirar de frente: la cara sigue hacia donde van los pies. Si
/// llega y no hay nadie, pierde el rastro y vuelve a patrullar solo.
/// Regresa si avanzó de verdad este frame.
fn investigate(enemy: &mut Enemy, maze: &Maze, block_size: usize, speed_multiplier: f32) -> bool {
    let Some(target_cell) = enemy.investigate_target else {
        enemy.state = State::Idle;
        return false;
    };

    let target_pos = pathfind::cell_center(target_cell, block_size);
    let distance = (target_pos - enemy.pos()).length();
    if distance < STOP_DISTANCE {
        // Llegó adonde se suponía que estaba, y no hay nadie: se le enfrió
        // la pista.
        enemy.investigate_target = None;
        enemy.path.clear();
        enemy.path_target = None;
        enemy.state = State::Idle;
        return false;
    }

    ensure_path_towards(enemy, maze, block_size, target_cell);
    if enemy.path.is_empty() {
        // Sin ruta a la última posición conocida (no debería pasar en un
        // laberinto conexo, pero si pasa no tiene caso quedarse esperando):
        // se rinde ya mismo en vez de congelarse ahí parado.
        enemy.investigate_target = None;
        enemy.state = State::Idle;
        return false;
    }

    let speed = CHASE_SPEED * speed_multiplier;
    follow_path(enemy, maze, block_size, speed)
}

/// Va de un punto al siguiente de `enemy.path`, consumiéndolos a medida que
/// los alcanza (varios en el mismo frame si el paso es más largo que la
/// distancia que falta), y orienta `facing` hacia el punto al que va.
/// Regresa si avanzó de verdad este frame.
fn follow_path(enemy: &mut Enemy, maze: &Maze, block_size: usize, speed: f32) -> bool {
    loop {
        let Some(next) = enemy.path.first().copied() else {
            return false;
        };

        let to_next = next - enemy.pos();
        let distance = to_next.length();
        if distance < WAYPOINT_TOLERANCE {
            enemy.path.remove(0);
            continue; // Ya está ahí: sigue con el siguiente punto del mismo frame.
        }

        enemy.facing = to_next.y.atan2(to_next.x);
        let (step_x, step_y) = (to_next.x / distance * speed, to_next.y / distance * speed);
        return move_with_wall_slide(enemy, maze, block_size, step_x, step_y);
    }
}

/// Patrulla solo: cada vez que llega a la celda que tenía como destino,
/// elige otra al azar entre las celdas libres vecinas (evitando, si hay más
/// de una opción, dar media vuelta hacia de dónde venía) y camina derecho
/// hacia su centro. Como sólo elige entre vecinas ya confirmadas libres, no
/// hay forma de que choque contra una pared. Regresa si avanzó de verdad
/// este frame.
fn wander(
    enemy: &mut Enemy,
    maze: &Maze,
    block_size: usize,
    speed_multiplier: f32,
    rng: &mut impl Rng,
) -> bool {
    if enemy.path.is_empty() {
        let current_cell = pathfind::to_cell(enemy.pos(), block_size);
        let mut candidates = pathfind::free_neighbors(maze, current_cell);

        if candidates.len() > 1 {
            if let Some(prev) = enemy.last_wander_cell {
                candidates.retain(|&c| c != prev);
            }
        }

        let Some(&choice) = candidates.get(rng.gen_range(0..candidates.len().max(1))) else {
            return false; // Celda aislada: no debería pasar en un laberinto conexo.
        };

        enemy.last_wander_cell = Some(current_cell);
        enemy.path = vec![pathfind::cell_center(choice, block_size)];
    }

    let speed = WANDER_SPEED * speed_multiplier;
    let moved = follow_path(enemy, maze, block_size, speed);
    if !moved {
        enemy.path.clear(); // Fuerza elegir otra celda el siguiente frame.
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

    /// Dos cuartos (celdas 1-3 y 5-9) separados por una pared en x=4, sin
    /// hueco: nunca hay línea de vista de un lado al otro.
    fn two_sealed_rooms() -> Maze {
        let cells: Vec<Vec<char>> = ["+++++++++++", "+   +     +", "+   +     +", "+++++++++++"]
            .iter()
            .map(|r| r.chars().collect())
            .collect();
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
        let maze = two_sealed_rooms();
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
    fn wandering_enemy_never_enters_a_wall_cell() {
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
    fn wandering_enemy_avoids_immediately_reversing_when_it_has_a_choice() {
        // Pasillo recto de tres celdas de ancho: parado en medio, sólo la
        // celda "de la que viene" debería quedar descartada.
        let maze = open_room(3, 15);
        let block = 20;
        let mut enemy = Enemy::new(Vector2::new(30.0, 150.0), 'e', block as f32, 0.0);
        enemy.last_wander_cell = Some((1, 6)); // como si acabara de bajar desde arriba

        let mut rng = StdRng::seed_from_u64(1);
        wander(&mut enemy, &maze, block, 1.0, &mut rng);

        assert_ne!(
            pathfind::to_cell(enemy.pos(), block),
            (1, 6),
            "no debería haber vuelto de inmediato a la celda de la que venía"
        );
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
    fn alert_makes_enemy_investigate_without_seeing_the_player() {
        let maze = open_room(15, 15);
        let block = 20;
        // Mirando al este; el jugador queda detrás (al oeste), fuera del
        // cono de visión.
        let mut enemies = vec![Enemy::new(Vector2::new(150.0, 150.0), 'e', block as f32, 0.0)];
        let player = Player::new(Vector2::new(40.0, 150.0), 0.0, PI / 3.0);
        assert!(!can_see_player(&enemies[0], &player, &maze, block));

        alert_all(&mut enemies, player.pos, block);
        update_enemies(&mut enemies, &player, &maze, block, 1.0);

        assert!(
            enemies[0].is_hunting(),
            "un enemigo alertado debería ir a investigar aunque no vea al jugador"
        );
        assert!(!enemies[0].is_chasing(), "investigar no es lo mismo que tenerlo a la vista");
    }

    #[test]
    fn investigating_enemy_paths_around_a_wall_towards_the_alert() {
        // Dos cuartos conectados por un hueco en la fila del medio: la
        // única forma de llegar del uno al otro en línea recta chocaría con
        // la pared de en medio, así que hace falta rodear.
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
        let block = 20;
        let mut enemies = vec![Enemy::new(Vector2::new(30.0, 30.0), 'e', block as f32, 0.0)];
        let player_pos = Vector2::new(170.0, 30.0);

        alert_all(&mut enemies, player_pos, block);
        let fake_player = Player::new(Vector2::new(-1000.0, -1000.0), 0.0, PI / 3.0); // lejos: nunca lo ve
        let mut closest = f32::INFINITY;
        for _ in 0..200 {
            update_enemies(&mut enemies, &fake_player, &maze, block, 1.0);
            assert!(
                maze.is_free(enemies[0].pos().x, enemies[0].pos().y, block, 0.0),
                "no debería terminar dentro de una pared siguiendo la ruta"
            );
            closest = closest.min((enemies[0].pos() - player_pos).length());
            if !enemies[0].is_hunting() {
                break; // Llegó, perdió el rastro y ya volvió a patrullar: no hace falta seguir.
            }
        }

        assert!(
            closest < 40.0,
            "debería haber llegado cerca de la última posición conocida en algún momento; más cerca que estuvo: {closest}"
        );
    }

    #[test]
    fn losing_the_trail_goes_back_to_idle_and_forgets_the_target() {
        let maze = two_sealed_rooms();
        let block = 20;
        let mut enemies = vec![Enemy::new(Vector2::new(40.0, 30.0), 'e', block as f32, 0.0)];
        let far_player = Player::new(Vector2::new(-1000.0, -1000.0), 0.0, PI / 3.0); // nunca lo ve

        // El objetivo de la alerta es su propia celda: ya llegó, así que
        // debería perder el rastro en la primera actualización.
        let own_pos = enemies[0].pos();
        alert_all(&mut enemies, own_pos, block);
        update_enemies(&mut enemies, &far_player, &maze, block, 1.0);

        assert!(
            !enemies[0].is_hunting(),
            "al llegar sin encontrar al jugador debería volver a patrullar"
        );
    }

    #[test]
    fn seeing_the_player_while_investigating_switches_to_chasing() {
        let maze = open_room(15, 15);
        let block = 20;
        let mut enemies = vec![Enemy::new(Vector2::new(150.0, 150.0), 'e', block as f32, 0.0)];
        let player = Player::new(Vector2::new(40.0, 150.0), 0.0, PI / 3.0);

        alert_all(&mut enemies, player.pos, block);
        update_enemies(&mut enemies, &player, &maze, block, 1.0); // investigando, aún no lo ve

        // Lo reorienta para que quede justo dentro del cono de visión.
        enemies[0].facing = (player.pos - enemies[0].pos()).y.atan2((player.pos - enemies[0].pos()).x);
        update_enemies(&mut enemies, &player, &maze, block, 1.0);

        assert!(enemies[0].is_chasing(), "al verlo debería pasar a perseguir de verdad");
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
