# Grav

A 3D game where the player places sources of attraction in a world that
reacts physically. Built on [Engine](https://github.com/ArndRosemeier/Engine).

The design is the specification: "Grav: world and concept design"
(https://claude.ai/artifact/1zJGPq7k5dQvqWErgep7u9). Rule numbers in the
code (A1, S9, W3, P6 ...) refer to it.

## Layout

| Path | What |
|---|---|
| `crates/grav-core` | The rules as plain data: attraction field (veins, heart, stones; six profiles, cutoff, inner ramp, shielding, lodestone charge rules), stones and coating, building bonds, the hero's body rules, and `Tuning` (every number, loadable from JSON). No engine, deterministic, so a server can own it later. |
| `crates/grav` | The sandbox: one spire of dead stone out of the outer wall (middle band around the spire, heavy wall face, a seam at the root, a light island above the tip, void between), bonded houses, stalls and loose goods, water (a pool beside the start and a lake hanging under the island, Engine 3D fluid boxes that follow the field, stones included), the hero, the stone verbs, dust, stone sense and the placing preview. |
| `tools/verify.py` | Runs the game as an Engine verify run (a scripted demo, screenshots, report) into `target/verify/grav/`. |

## Play

```bash
cargo run --release -p grav
```

WASD walk, Q/E turn (or the mouse with the right button held), Space jump
(push off while drifting), 1-9 pick a stone, F hold to open, C hold to close
(costs coating), G take in hand / pack, T hold to aim (preview: landing point,
your fall arrow, pieces that would break in red) and release to throw, V set
on the surface in reach, X recover, Z recall (spring sheath), R hold for stone
sense, P sandbox palette (dress any stone, unlimited coating).

The stone pool to the right of where you start is real 3D water: open a
stone over it and the water climbs towards it; the lake under the island is
held up by the island's vein. (The hero walks through water for now: no
swimming yet.)

Numbers come from `grav_core::Tuning`; put a `tuning.json` next to where you
run the game to override any of them.

## Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python tools/verify.py grav --frames 480 --shots 60,150,420
```
