use raylib::prelude::*;

/// Qué tan lejos (en pixeles) alcanza la "linterna" del jugador antes de que
/// una pared se vea prácticamente negra.
const TORCH_RANGE: f32 = 320.0;

/// Brillo mínimo aunque algo esté fuera del alcance de la linterna: nunca
/// baja a negro puro, para no perder del todo la silueta del laberinto.
const AMBIENT: f32 = 0.05;

/// Qué tanto oscurece el viñeteado los bordes de la pantalla respecto al
/// centro (0 = sin viñeteado, 1 = los bordes casi no reciben luz). Simula
/// que la linterna es un cono apuntando al frente, no una luz que ilumina
/// parejo todo lo que entra en el FOV.
const VIGNETTE_STRENGTH: f32 = 0.5;

/// Intensidad de luz (0.0..1.0) de la linterna del jugador sobre un punto a
/// `distance` pixeles de la cámara y a `angle_offset` radianes del centro de
/// la vista (0 = justo al centro, ±`half_fov` = borde de la pantalla).
///
/// Combina dos cosas:
/// - Caída por distancia al cuadrado (no lineal): una linterna real ilumina
///   fuerte cerca y se apaga rápido, no de forma pareja.
/// - Un viñeteado que oscurece los bordes de la pantalla, como el cono real
///   de una linterna en vez de una luz que llega igual a todo el FOV.
pub fn torch_intensity(distance: f32, angle_offset: f32, half_fov: f32) -> f32 {
    let falloff = (1.0 - (distance / TORCH_RANGE)).clamp(0.0, 1.0);
    let falloff = falloff * falloff;

    let edge = if half_fov > 0.0 {
        (angle_offset / half_fov).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let vignette = 1.0 - edge * edge * VIGNETTE_STRENGTH;

    (AMBIENT + (1.0 - AMBIENT) * falloff * vignette).clamp(AMBIENT, 1.0)
}

/// Color casi negro con un dejo azulado hacia el que se funden las sombras,
/// en vez de simplemente multiplicar hacia negro puro: un negro neutro se
/// siente "apagado", mientras que un negro frío se siente más como sombra
/// real y refuerza el tono opresivo de la ambientación.
const SHADOW_TINT: Color = Color::new(6, 8, 14, 255);

/// Aplica una intensidad de luz (0.0..1.0) a un color, mezclándolo hacia
/// `SHADOW_TINT` en vez de hacia negro plano.
pub fn apply(color: Color, intensity: f32) -> Color {
    let i = intensity.clamp(0.0, 1.0);
    Color::new(
        lerp_channel(SHADOW_TINT.r, color.r, i),
        lerp_channel(SHADOW_TINT.g, color.g, i),
        lerp_channel(SHADOW_TINT.b, color.b, i),
        color.a,
    )
}

fn lerp_channel(shadow: u8, lit: u8, t: f32) -> u8 {
    (shadow as f32 + (lit as f32 - shadow as f32) * t).round() as u8
}
