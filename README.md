# Grav

A 3D game where the player places sources of attraction in a world that
reacts physically. Built on [Engine](https://github.com/ArndRosemeier/Engine).

The design is the specification: "Grav: world and concept design"
(https://claude.ai/artifact/1zJGPq7k5dQvqWErgep7u9). Rule numbers in the
code (A1, S9, W3, P6 ...) refer to it.

## Layout

| Path | What |
|---|---|
| `crates/grav-core` | The rules as plain data: attraction field (veins, heart, stones; six profiles, cutoff, inner ramp, shielding, lodestone charge rules), stones in their timed boxes (one-sided fields), building bonds, the hero's body rules, and `Tuning` (every number, loadable from JSON). No engine, deterministic, so a server can own it later. |
| `crates/grav` | The sandbox: one spire of dead stone out of the outer wall (middle band around the spire, heavy wall face, a seam at the root, a light island above the tip, void between), bonded houses, stalls and loose goods, water (a pool beside the start and a lake hanging under the island, Engine 3D fluid boxes that follow the field, stones included), the hero, the boxes (belt, bench, wind-up throw and its preview), dust and stone sense. |
| `tools/get_packs.py` | Downloads the Actors packs (bodies, clothes, clips) into `assets/packs` (git-ignored) for the dwarf hero. |
| `tools/dupes.py` | Looks for duplicated code with Arnd's Toolbox (functions written twice, pasted snippets); reports in `target/dupes/`. Needs Node. |
| `tools/verify.py` | Runs the game as an Engine verify run (a scripted demo, screenshots, report) into `target/verify/grav/`. |

## Play

```bash
python tools/get_packs.py   # once: the Actors packs (about 450 MB) into assets/packs
cargo run --release -p grav
```

`measure.bat` (or `GRAV_MEASURE=1 ENGINE_PACE=0 cargo run --release -p grav`) measures the
frame rate from five fixed views, with everything drawn and then with grass,
plants, buildings and the far cavern taken away in turn, and writes what each
costs to `measure.txt`. `GRAV_FRAME_TIMES=1` shows frame times while playing.

With the packs the hero is an Actors dwarf that idles, walks and runs;
without them a block figure stands in (`GRAV_PACKS` points elsewhere).

WASD walk, Shift sprint, Q/E turn (or drag with the right mouse button to
look around), Space jump (push off while drifting), R hold for stone sense.

Stones travel in coated boxes. 1-9 takes a box in hand (the same key puts it
away). With a box in hand, a glass box shows where it would hang, with fans
over the sides it will open, your fall arrow and the pieces that would break.
+ and - set how far it flies (hold to keep changing it; the range shows
under the crosshair), and a left click throws it. The box flies straight, shut and weightless, and air
stops it. Tab opens the box bench:
click a face on the unfolded box to make it open some seconds after the box
comes to rest, and set when the box bares its mass and comes down. Walk to a
box that has come down to pick it up. The belt at the bottom shows every box,
and a ring over a hanging box counts down to its drop.

The spring by the start is real 3D water: hang a box over it with its down
face open and the water climbs towards it. (The hero walks through water for
now: no swimming yet.)

Numbers come from `grav_core::Tuning`; put a `tuning.json` next to where you
run the game to override any of them.

## Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
python tools/test.py
```

`tools/test.py` runs the essential tests: the ones that catch game-breaking bugs
(attraction, the hero, stones, bonds, which way is down). Each has a budget of
10 ms, which the script measures and enforces. Every other test is marked
`#[ignore = "full"]` and runs only on demand, with no time budget:

```bash
python tools/test.py --full
python tools/verify.py grav --frames 600 --shots 60,150,420,560
```

The verify run (the scripted demo) is on demand too: under a minute on a desktop
GPU, about 25 minutes on a software renderer.

A screenshot tour of the world (no hero camera, no HUD; a new view every 12 frames):

```bash
GRAV_TOUR=1 python tools/verify.py grav --frames 132 --shots 10,22,34,46,58,70,82,94,106,118
```
