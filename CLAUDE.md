# Grav: notes for coding agents

- The design document is the source of truth: "Grav: world and concept design", https://claude.ai/artifact/1zJGPq7k5dQvqWErgep7u9 (a Claude Doc). Do not invent answers for anything in its "Parked and out of scope" section.
- Arnd wants every change pushed straight to `main` (he does not do git himself). No pull requests waiting for review.
- `crates/grav-core` stays engine-free and deterministic: no `engine`, no renderer, no unseeded randomness. Game code that needs the engine goes in the game crate.
- Numbers to tune belong in `grav_core::Tuning` (JSON-loadable), not as constants in game code.
- Engine changes go to https://github.com/ArndRosemeier/Engine (its own CI: fmt, clippy, tests, verify runs, a 0.001 version bump per change). Read its AGENTS.md before changing it.
- Checks before pushing: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
