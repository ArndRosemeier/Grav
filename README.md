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

## Checks

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
