use raylib::prelude::*;

/// Todos los efectos y música cargados una sola vez al arrancar. Igual que
/// con `TextureManager`, cada campo es `Option`: si un archivo no está o no
/// se pudo cargar, esa pista simplemente queda en silencio en vez de tronar
/// el juego.
///
/// `Sound`/`Music` están atados por lifetime al `RaylibAudio` del que salen
/// (`'aud`), así que `Sfx` sólo puede vivir mientras el dispositivo de audio
/// siga abierto: se construye pasándole `&'aud RaylibAudio` una sola vez en
/// `main`.
pub struct Sfx<'aud> {
    pub player_footstep: Option<Sound<'aud>>,
    pub enemy_footstep: Option<Sound<'aud>>,
    pub first_totem_destroyed: Option<Sound<'aud>>,
    pub first_totem_enemy_sound: Option<Sound<'aud>>,
    pub any_totem_destroyed: Option<Sound<'aud>>,
    pub enemy_scream_any_totem: Option<Sound<'aud>>,
    pub last_totem_destroyed: Option<Sound<'aud>>,
    /// Ambiente mientras no ha despertado el enemigo (antes de romper el
    /// primer tótem).
    pub ambient_calm: Option<Music<'aud>>,
    /// Ambiente desde que despierta el enemigo hasta el final de la partida.
    pub ambient_tense: Option<Music<'aud>>,
    pub menu_move: Option<Sound<'aud>>,
    pub menu_select: Option<Sound<'aud>>,
    pub menu_theme: Option<Music<'aud>>,
}

impl<'aud> Sfx<'aud> {
    pub fn load(device: &'aud RaylibAudio) -> Self {
        Sfx {
            player_footstep: load_sound(device, "assets/player_footstep.mp3"),
            enemy_footstep: load_sound(device, "assets/enemy_footstep.mp3"),
            first_totem_destroyed: load_sound(device, "assets/first_totem_destroyed.mp3"),
            first_totem_enemy_sound: load_sound(device, "assets/first_totem_enemy_sound.mp3"),
            any_totem_destroyed: load_sound(device, "assets/any_totem_destroyed.mp3"),
            enemy_scream_any_totem: load_sound(device, "assets/enemy_scream_any_totem.mp3"),
            last_totem_destroyed: load_sound(device, "assets/last_totem_destroyed.mp3"),
            ambient_calm: load_music(device, "assets/ambient_1.mp3"),
            ambient_tense: load_music(device, "assets/ambient_2.mp3"),
            menu_move: load_sound(device, "assets/menu_move.mp3"),
            menu_select: load_sound(device, "assets/menu_select.mp3"),
            menu_theme: load_music(device, "assets/menu_theme.mp3"),
        }
    }

    pub fn play_player_footstep(&self) {
        play(&self.player_footstep);
    }

    pub fn play_enemy_footstep(&self) {
        play(&self.enemy_footstep);
    }

    /// Se dispara al romper un tótem que no es ni el primero ni el último:
    /// el estruendo del tótem más el grito del enemigo, superpuestos (igual
    /// que `play_first_totem_broken`, pero con el grito de "sigo despierto"
    /// en vez del de "acabo de despertar").
    pub fn play_any_totem_destroyed(&self) {
        play(&self.any_totem_destroyed);
        play(&self.enemy_scream_any_totem);
    }

    pub fn play_last_totem_destroyed(&self) {
        play(&self.last_totem_destroyed);
    }

    /// Se dispara al romper el primer tótem: el estruendo del propio tótem
    /// más el "grito" de lo que acaba de despertar, superpuestos.
    pub fn play_first_totem_broken(&self) {
        play(&self.first_totem_destroyed);
        play(&self.first_totem_enemy_sound);
    }

    /// Corta el ambiente tranquilo y arranca el tenso. Se llama una sola vez,
    /// justo cuando se rompe el primer tótem.
    pub fn switch_to_tense_ambient(&self) {
        stop_music(&self.ambient_calm);
        play_music(&self.ambient_tense);
    }

    /// Arranca el ambiente tranquilo. Se llama una sola vez al iniciar la
    /// partida.
    pub fn start_calm_ambient(&self) {
        play_music(&self.ambient_calm);
    }

    /// Corta cualquiera de los dos ambientes de partida que esté sonando.
    /// Se llama al salir de un nivel (se gane, se pierda, o se vuelva al
    /// menú), para no dejarlo sonando de fondo mientras suena la música del
    /// menú.
    pub fn stop_gameplay_ambient(&self) {
        stop_music(&self.ambient_calm);
        stop_music(&self.ambient_tense);
    }

    pub fn play_menu_move(&self) {
        play(&self.menu_move);
    }

    pub fn play_menu_select(&self) {
        play(&self.menu_select);
    }

    pub fn start_menu_theme(&self) {
        play_music(&self.menu_theme);
    }

    pub fn stop_menu_theme(&self) {
        stop_music(&self.menu_theme);
    }

    /// Hay que llamarlo una vez por frame para que el streaming de música no
    /// se corte a media canción: `Music` sólo lee del archivo hacia
    /// adelante bajo demanda, no precarga todo a RAM como `Sound`.
    pub fn update_streams(&self) {
        if let Some(m) = &self.ambient_calm {
            m.update_stream();
        }
        if let Some(m) = &self.ambient_tense {
            m.update_stream();
        }
        if let Some(m) = &self.menu_theme {
            m.update_stream();
        }
    }
}

fn load_sound<'aud>(device: &'aud RaylibAudio, path: &str) -> Option<Sound<'aud>> {
    match device.new_sound(path) {
        Ok(sound) => Some(sound),
        Err(err) => {
            eprintln!("No se pudo cargar el sonido '{path}': {err}. Se queda en silencio.");
            None
        }
    }
}

fn load_music<'aud>(device: &'aud RaylibAudio, path: &str) -> Option<Music<'aud>> {
    match device.new_music(path) {
        Ok(music) => Some(music),
        Err(err) => {
            eprintln!("No se pudo cargar la música '{path}': {err}. Se queda en silencio.");
            None
        }
    }
}

fn play(sound: &Option<Sound>) {
    if let Some(s) = sound {
        s.play();
    }
}

fn play_music(music: &Option<Music>) {
    if let Some(m) = music {
        m.play_stream();
    }
}

fn stop_music(music: &Option<Music>) {
    if let Some(m) = music {
        m.stop_stream();
    }
}
