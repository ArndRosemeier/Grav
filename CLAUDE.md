# Grav: notes for coding agents

- The design document is the source of truth: "Grav: world and concept design", https://claude.ai/artifact/1zJGPq7k5dQvqWErgep7u9 (a Claude Doc). Do not invent answers for anything in its "Parked and out of scope" section.
- Arnd wants every change pushed straight to `main` (he does not do git himself). No pull requests waiting for review.
- `crates/grav-core` stays engine-free and deterministic: no `engine`, no renderer, no unseeded randomness. Game code that needs the engine goes in the game crate.
- Numbers to tune belong in `grav_core::Tuning` (JSON-loadable), not as constants in game code.
- Engine changes go to https://github.com/ArndRosemeier/Engine (its own CI: fmt, clippy, tests, verify runs, a 0.001 version bump per change). Read its AGENTS.md before changing it.
- One mechanic, one implementation (Arnd, 2026-11-16): duplicated mechanics drift apart when the rules change. Before adding a helper or a rule, look for an existing one; run `python tools/dupes.py` after larger changes and merge any new copy it finds.
- Checks before pushing: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && python tools/test.py`.
- Tests (Arnd, 2026-11-16): only essential tests (they catch game-breaking bugs) run on every build, and each has a 10 ms budget that `tools/test.py` enforces; one over it needs `// Over budget: <reason>` above its `#[test]`. Every other test is `#[ignore = "full"]` and runs only when asked for (`python tools/test.py --full`, no budget). A new test is full unless it guards against a game-breaking bug and fits the budget. The demo verify run (`tools/verify.py`) is on demand too.

## Quality bar (Arnd, 2026-11-15; taken from HeroWorld's AGENTS.md)

Arnd aims very high for graphical fidelity and judges the game by its screenshots. The first
sandbox (smooth cylinder spire, box houses, flat light) was a mechanics and engine test, not
the bar: do not copy it, and do not build new things "like the existing ones".

- **Final quality, never stand-ins.** Everything the player sees (rock, buildings, props,
  characters, water, stones) ships at the quality of a modern game. A boxy model is a bug, not
  a first version. If something cannot be final yet, say so; do not ship a placeholder quietly.
- **Shape:** curved things are smooth, rock looks like rock (Engine's rock and terrain
  generators, voxel terrain, AssetGenerator `.glb`), not primitives. Hard edges only where the
  real thing has them. Round parts get at least 24 sides near the camera.
- **Close-up budget:** spend what a close-up needs (a few hundred to a couple of thousand
  triangles for a small prop, several thousand for a large one), with real materials: textures,
  roughness, normal detail, wear.
- **Frame rate comes from LODs and instancing,** not from crude meshes: at least three levels
  per model (`spawn_lod_at`). Measure before and after a speed change.
- **Characters are Actors** characters (clothes, faces, animation), never block figures.
- **Light and air are part of the look:** the heart as the light source, lamps, shadows, haze,
  bloom. A scene lit flat is not done.
- **What is drawn is what collides:** a thing's solid shape comes from the same data as its
  mesh.
- **Work that takes time runs on a worker thread** (generation, meshing, textures); the main
  thread only hands finished results to Engine.
- **Look before you push:** take a close-up screenshot (camera within 5 m) of every new or
  changed thing and look at it. If it would look out of place in a modern game, it is not done.
- **Generic needs go to Engine** (rendering, physics, streaming, destruction), not worked
  around in the game.
- **Frame rate:** 60 FPS with vsync off on Arnd's PC is the lower end, not the goal (his card
  is above average, so players' machines will be slower).
- Warnings are errors. Do not silence a lint, skip a test or loosen a check to get green.
