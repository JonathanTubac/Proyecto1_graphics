use crate::audio::Sfx;
use raylib::prelude::*;

/// Qué pantalla del menú se está mostrando ahora mismo.
#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Main,
    LevelSelect,
    Instructions,
}

/// Qué pasó este frame en el menú: nada, arrancar la partida en tal nivel
/// (índice dentro de la lista de niveles), o cerrar la aplicación.
pub enum Outcome {
    None,
    StartGame(usize),
    Quit,
}

const MAIN_OPTIONS: [&str; 3] = ["Jugar", "Seleccionar nivel", "Instrucciones"];

pub struct Menu {
    pub screen: Screen,
    pub main_selected: usize,
    pub level_selected: usize,
}

impl Menu {
    pub fn new() -> Self {
        Menu {
            screen: Screen::Main,
            main_selected: 0,
            level_selected: 0,
        }
    }

    /// Procesa el input de este frame y regresa qué pasó. Dibujar (`draw`,
    /// más abajo) va aparte: este método sólo cambia el estado.
    pub fn update(&mut self, window: &RaylibHandle, sfx: &Sfx, level_count: usize) -> Outcome {
        match self.screen {
            Screen::Main => self.update_main(window, sfx),
            Screen::LevelSelect => self.update_level_select(window, sfx, level_count),
            Screen::Instructions => self.update_instructions(window, sfx),
        }
    }

    fn update_main(&mut self, window: &RaylibHandle, sfx: &Sfx) -> Outcome {
        navigate(window, &mut self.main_selected, MAIN_OPTIONS.len(), sfx);

        if confirm_pressed(window) {
            sfx.play_menu_select();
            return match self.main_selected {
                0 => Outcome::StartGame(self.level_selected),
                1 => {
                    self.screen = Screen::LevelSelect;
                    Outcome::None
                }
                2 => {
                    self.screen = Screen::Instructions;
                    Outcome::None
                }
                _ => Outcome::None,
            };
        }

        if window.is_key_pressed(KeyboardKey::KEY_ESCAPE) {
            return Outcome::Quit;
        }

        Outcome::None
    }

    fn update_level_select(&mut self, window: &RaylibHandle, sfx: &Sfx, level_count: usize) -> Outcome {
        navigate(window, &mut self.level_selected, level_count, sfx);

        if confirm_pressed(window) {
            sfx.play_menu_select();
            return Outcome::StartGame(self.level_selected);
        }

        if window.is_key_pressed(KeyboardKey::KEY_ESCAPE) {
            sfx.play_menu_select();
            self.screen = Screen::Main;
        }

        Outcome::None
    }

    fn update_instructions(&mut self, window: &RaylibHandle, sfx: &Sfx) -> Outcome {
        if window.is_key_pressed(KeyboardKey::KEY_ESCAPE) || confirm_pressed(window) {
            sfx.play_menu_select();
            self.screen = Screen::Main;
        }
        Outcome::None
    }
}

fn confirm_pressed(window: &RaylibHandle) -> bool {
    window.is_key_pressed(KeyboardKey::KEY_ENTER) || window.is_key_pressed(KeyboardKey::KEY_SPACE)
}

/// Mueve `selected` con flechas o W/S (arriba/abajo), en ciclo dentro de
/// `count` opciones, y reproduce el sonido de moverse si de verdad cambió.
fn navigate(window: &RaylibHandle, selected: &mut usize, count: usize, sfx: &Sfx) {
    if count == 0 {
        return;
    }
    let up = window.is_key_pressed(KeyboardKey::KEY_UP) || window.is_key_pressed(KeyboardKey::KEY_W);
    let down = window.is_key_pressed(KeyboardKey::KEY_DOWN) || window.is_key_pressed(KeyboardKey::KEY_S);

    if up {
        *selected = (*selected + count - 1) % count;
        sfx.play_menu_move();
    } else if down {
        *selected = (*selected + 1) % count;
        sfx.play_menu_move();
    }
}

/// Dibuja la pantalla de menú activa. Se llama con las primitivas de raylib
/// directo (no con el framebuffer de software), igual que las pantallas de
/// fin de partida: necesita texto, y el framebuffer no sabe dibujarlo.
/// `draw_text` no soporta UTF-8, así que todo va sin acentos.
pub fn draw(d: &mut RaylibDrawHandle, width: i32, height: i32, menu: &Menu, level_names: &[&str]) {
    d.draw_rectangle(0, 0, width, height, Color::new(8, 7, 10, 255));

    match menu.screen {
        Screen::Main => draw_main(d, width, height, menu.main_selected),
        Screen::LevelSelect => draw_level_select(d, width, height, menu.level_selected, level_names),
        Screen::Instructions => draw_instructions(d, width, height),
    }
}

fn draw_title(d: &mut RaylibDrawHandle, width: i32, text: &str, y: i32, size: i32, color: Color) {
    let w = d.measure_text(text, size);
    d.draw_text(text, width / 2 - w / 2, y, size, color);
}

fn draw_options(d: &mut RaylibDrawHandle, width: i32, start_y: i32, options: &[&str], selected: usize) {
    let size = 32;
    let spacing = 50;
    for (i, opt) in options.iter().enumerate() {
        let (label, color) = if i == selected {
            (format!("> {opt} <"), Color::new(230, 70, 70, 255))
        } else {
            (opt.to_string(), Color::new(190, 190, 195, 255))
        };
        let w = d.measure_text(&label, size);
        d.draw_text(&label, width / 2 - w / 2, start_y + i as i32 * spacing, size, color);
    }
}

fn draw_footer(d: &mut RaylibDrawHandle, width: i32, height: i32, text: &str) {
    let size = 18;
    let w = d.measure_text(text, size);
    d.draw_text(text, width / 2 - w / 2, height - 40, size, Color::new(150, 150, 155, 255));
}

fn draw_main(d: &mut RaylibDrawHandle, width: i32, height: i32, selected: usize) {
    draw_title(d, width, "MAZETOT", 90, 64, Color::new(200, 40, 50, 255));
    draw_options(d, width, 280, &MAIN_OPTIONS, selected);
    draw_footer(d, width, height, "Flechas o W/S: moverse | ENTER: elegir | ESC: salir");
}

fn draw_level_select(d: &mut RaylibDrawHandle, width: i32, height: i32, selected: usize, level_names: &[&str]) {
    draw_title(d, width, "SELECCIONA NIVEL", 90, 48, Color::new(200, 180, 60, 255));
    draw_options(d, width, 280, level_names, selected);
    draw_footer(d, width, height, "Flechas o W/S: moverse | ENTER: jugar | ESC: volver");
}

fn draw_instructions(d: &mut RaylibDrawHandle, width: i32, height: i32) {
    draw_title(d, width, "INSTRUCCIONES", 60, 44, Color::new(200, 180, 60, 255));

    let lines: [&str; 15] = [
        "OBJETIVO",
        "Destruye todos los totems y llega a la puerta de salida.",
        "El enemigo despierta al primer totem y se acelera con cada uno.",
        "Cada totem que rompas lo manda a buscar donde estabas: escondete",
        "en un locker (E) si tienes uno cerca, ahi no puede verte ni tocarte.",
        "",
        "CONTROLES",
        "W / S      Avanzar / retroceder",
        "A / D      Moverse de lado",
        "SHIFT      Correr (gasta energia)",
        "MOUSE      Girar la camara",
        "E          Destruir el totem cercano, o entrar/salir de un locker",
        "M          Ver el mapa completo",
        "N          Mostrar u ocultar el minimapa",
        "TAB        Soltar o atrapar el mouse",
    ];

    let size = 22;
    let start_y = 140;
    let x = 100;
    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let color = if *line == "OBJETIVO" || *line == "CONTROLES" {
            Color::new(230, 200, 90, 255)
        } else {
            Color::new(210, 210, 215, 255)
        };
        d.draw_text(line, x, start_y + i as i32 * 28, size, color);
    }

    draw_footer(d, width, height, "ENTER o ESC: volver");
}
