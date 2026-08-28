# Mazetot

A small first-person horror game written in Rust with [raylib](https://www.raylib.com/), built
on a custom raycasting engine (the same technique Wolfenstein 3D used): the world is a 2D grid
of characters, and the "third dimension" comes from casting one ray per screen column and
turning the distance to the wall into that column's height. Everything is drawn pixel by pixel
onto a software framebuffer in RAM; raylib is only used to open the window, read keyboard/mouse
input, play audio, and upload that framebuffer to the screen as a texture.

On top of that engine there's a full little game: a menu with 3 levels, totems you have to
destroy, an enemy that hunts you with real AI (vision + memory of your last known position +
pathfinding), lockers built into the walls to hide from it, a flashlight as the only light
source, and dynamic audio (footsteps, ambience, and hums that change volume with distance).

## Video

[![Mazetot video](https://img.youtube.com/vi/a3fiC9cPOMk/hqdefault.jpg)](https://youtu.be/a3fiC9cPOMk)

[▶ Watch on YouTube](https://youtu.be/a3fiC9cPOMk)

## How to run it

```bash
cargo run           # debug mode, runs fine at 60 fps
cargo run --release # for more headroom
```

Run it from the project root: that's where `assets/` (textures and audio) and the
`maze1.txt` / `maze2.txt` / `maze3.txt` files loaded by the level-select menu live.

If a file is missing from `assets/` (a texture or a sound), the game doesn't crash: it
generates a placeholder, or that track simply plays silently.

## Controls

**Menu**

| Key | Action |
|---|---|
| Arrows / `W` `S` | Move between options |
| `ENTER` / `SPACE` | Select |
| `ESC` | Go back a screen, or quit from the main menu |

**In a level**

| Key | Action |
|---|---|
| `W` / `S` | Move forward / backward |
| `A` / `D` | Strafe |
| Mouse | Turn the camera |
| `SHIFT` | Run (drains stamina) |
| `E` | Destroy the nearby totem, or enter/exit a locker |
| `M` | View the full map |
| `N` | Toggle the minimap |
| `TAB` | Release or capture the mouse |
| `F1` | Save a screenshot to `maze.png` |
| `ESC` | Back to the menu (after winning or losing) |

## How to play

Each level has several totems scattered around the maze. Breaking the first one (`E` near it)
wakes up the level's single enemy; every totem after that speeds it up a bit more. The exit
door (`g`) won't open until every totem is down.

While it can't see you, the enemy patrols on its own. If it spots you, it gives real chase.
Every time a totem breaks, the enemy heads straight for wherever you were standing at that
moment, even without seeing you — as if it heard the noise. If it gets there and finds no one,
it loses the trail and goes back to patrolling. Hiding in a nearby locker (`E`) is how you make
that happen: while you're inside, the enemy can neither see nor touch you.

## The maze file

`maze1.txt`, `maze2.txt` and `maze3.txt` are plain text where each character is a
`BLOCK_SIZE`-pixel cell:

| Character | Meaning |
|---|---|
| `+` `-` `\|` | Wall |
| space | Floor |
| `p` | Player start position |
| `g` | Exit (won't open until every totem is destroyed) |
| `t` | Totem to destroy |
| `e` | Where the enemy spawns (once the first totem breaks) |
| `l` | Locker: counts as a wall (blocks movement) and is used from the cell next to it |

```
+--+--+--+--+
|p          |
+  +--+  +  +
|  t     |  |
+  +  +--+--+
|  |    l   |
+  +--+--+  +
|   e    | g|
+--+--+--+--+
```

You can edit or add a new map: the window sizes itself to the largest of the three files
(`width_in_characters * BLOCK_SIZE`). Rows don't need to be the same length — trailing spaces
that editors strip are treated as floor.

## Structure

```
src/
  main.rs         Window, menu/level loop, 3D raycasting, floor/ceiling, HUD
  menu.rs         Main menu, level selector, instructions screen
  maze.rs         Maze type: loads the file, collisions, line of sight
  caster.rs       cast_ray: DDA, returns where and what each ray hit
  player.rs       Player (position, angle, fov, health, stamina) and its input
  enemy.rs        Enemy AI: vision, states (patrol/investigate/chase)
  pathfind.rs     Shortest path between two cells (BFS over the grid)
  totem.rs        Destructible totems and their proximity hum
  locker.rs       Wall-mounted locker; hiding from sight/damage
  sprites.rs      Billboards (enemy, totems, door) with z-buffer occlusion
  textures.rs     TextureManager + procedural texture/sprite generation
  lighting.rs     The "flashlight" model: distance falloff + vignette
  audio.rs        Sound effects and music; distance-based dynamic volume
  framebuffer.rs  Custom pixel buffer, uploaded to the window as a texture
```

## How it works

**The ray.** `cast_ray` uses DDA (Digital Differential Analysis): instead of stepping pixel by
pixel, it jumps from grid line to grid line until it hits a wall cell, which is both faster and
gives the exact impact position (no rounding to 1px), needed to texture without artifacts. It
returns an `Intersect` with the distance, the character it hit, `wall_x` (where along that face,
to pick the texture column) and whether it was a horizontal or vertical wall (so they're shaded
differently, which is what makes corners readable).

**The fan.** Rays are spread across the field of view (`fov = π/3`), starting at `a - fov/2`
and stepping a fraction of the fov per ray. The 2D map draws 120, the minimap 40, and the 3D
view casts one per screen column.

**The projection.** Each distance becomes a vertical wall column centered on the middle of the
screen:

```rust
let d = intersect.distance * (a - player.a).cos();
let stake_height = (BLOCK_SIZE as f32 / d) * distance_to_plane;
```

The `cos(a - player.a)` corrects the fisheye effect (rays at the edges of the fov travel more
distance than the ones at the center). `distance_to_plane = (width/2) / tan(fov/2)` instead of a
hand-tuned constant, so changing the fov doesn't require re-tuning the wall scale.

**Floor and ceiling.** Painted row by row (not column by column): unlike walls, an entire
horizontal row sits at the same real distance from the camera regardless of column (standard
floor-casting), so it can be darkened with the same flashlight model without repeating the
calculation per pixel.

**The sprites.** Enemy, totems and door are billboards: they always face the player. They're
drawn back-to-front using the same z-buffer the walls fill in, so a sprite partially behind a
corner gets clipped correctly instead of drawing over what should be hiding it.

**The flashlight.** Everything is lit with quadratic distance falloff plus a vignette that
darkens the edges of the screen (a real flashlight's cone, not an even light across the whole
fov), blending the color toward an almost-black shadow tone instead of flat black.

**The enemy AI.** Three states: `Idle` (patrolling), `Investigating` (heading to the last cell
where the player was known to be) and `Chasing` (has line of sight right now). Both chasing and
investigating compute their route with a BFS over the maze grid (`pathfind.rs`) — since every
cell costs the same, a BFS visits nodes in the same order a Dijkstra would, just without the
priority queue that algorithm needs for uneven weights — so the enemy routes around walls
instead of getting stuck on corners. Free patrol also moves cell to cell between already-confirmed
walkable neighbors, so it can never walk into a wall.

## Screenshots

These are from a fairly early build of the project (before textures, lighting, totems and the
enemy), but they're useful to see the raw raycasting technique: the 2D map with the ray fan and
the resulting 3D projection. For what the game looks like now, see the [video](#video) above.

![3D view](capturas/vista3d.png)

![2D map](capturas/mapa2d.png)

## Tests

```bash
cargo test
```

Doesn't open a window. Covers the raycaster's trigonometry (`cast_ray`, `wall_x`), the maze's
collisions and line of sight, the enemy AI (vision, patrolling without collisions, chasing and
investigating with and without obstacles in the way, losing the trail), pathfinding
(`pathfind.rs`), totems (proximity, destruction), lockers, and the player's health/stamina
handling.
