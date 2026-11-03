# Grey City

An ASCII roguelike life simulator. You are one person in a grey, slightly
wrong city of ~45 people who all have jobs, homes, rent, needs, grudges and
memories. Work your shift, pay rent, get promoted, or let one bad night and a
ringing payphone turn you into the reason the evening news exists.

Turn-based (1 turn = 1 minute), permadeath, one continuous life.

## Running

Needs Rust (stable). On Linux you also need the usual GL/X11 dev libraries
(`libx11-dev libxi-dev libgl1-mesa-dev`); macOS and Windows work out of the box.

```bash
cargo run --release -p greycity
```

### NixOS / Nix

The repo is a flake. Run it straight from GitHub:

```bash
nix run github:RustyBridges/greycity
```

Or from a checkout: `nix run` (game), `nix run .#headless -- --days 7`
(simulation log), `nix build` (installs to `./result`), and `nix develop`
(a dev shell with cargo, rust-analyzer and the GL/X11 libraries set up, so
`cargo run --release -p greycity` works). On NixOS this needs
`hardware.graphics.enable = true;` (usually already on with any desktop).
Under Wayland the game runs through XWayland.

Headless city simulation (no window, player on autopilot, prints every event):

```bash
cargo run --release -p greycity-sim --bin headless -- --seed 7 --days 7
```

Tests:

```bash
cargo test --release -p greycity-sim
```

## Controls

| Key | Action |
| --- | --- |
| Arrows / numpad / `hjklyubn` | Move. Bump people to interact, bump counters/registers to buy (or rob), bump a ringing payphone to answer |
| `t` + dir / `a` + dir | Talk / attack |
| `f` + dir | Set something on fire (needs a lighter) |
| `.` `s` numpad 5 | Wait 1 minute |
| `w` / `W` | Wait 15 minutes / 1 hour |
| `z` | Sleep |
| `e` | Work (at your highlighted station, during your shift) |
| `i` | Pockets |
| `c` | Character: relationships, memories, past "business" |
| `x` or mouse hover | Look |
| PgUp / PgDn | Scroll log |
| `+` / `-` | Zoom |
| Esc | Cancel / stop waiting |

## Layout

```
sim/            pure simulation, no graphics (deterministic per seed)
  data/jobs.ron   every job in the city: tier, shift, wage, promotion ladder
  data/weird.ron  the strange events the city rolls each hour
  src/map.rs      city generator: 4x3 blocks of tenements, offices, a bar, a park...
  src/world.rs    the World, the clock, economy (wages, rent, eviction, promotion), crime
  src/ai.rs       NPC decisions: work, sleep, eat, drink, socialize, flee, fight, chase
  src/social.rs   chat, insults, gossip/rumours, smoking
  src/combat.rs   body parts, bleeding, knockouts, arrests, jail
  src/fire.rs     fire spread through fuel, burning people, fire department
  src/events.rs   weird events, the payphone, grudge offers and objectives
  src/player.rs   every player action
game/           macroquad window: renderer, UI, input
```

Debugging aid: `GC_TRACE="Full Name"` prints one NPC's decisions every 30
minutes when running the headless sim. `GC_DEMO=<minutes>` starts the game
with a random life already lived on autopilot for that many minutes.
