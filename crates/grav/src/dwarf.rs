//! The hero's body: an Actors dwarf, when the packs are there
//! (`python tools/get_packs.py`). Without them the hero stays a block
//! figure, so the game and its checks run anywhere.

use actors_engine::{BuildOptions, CharacterBuilder};
use actors_generator::{AllowOptions, Gender, RaceChoiceOption, RandomOptions};
use actors_human::RaceId;
use engine::prelude::*;
use std::path::Path;

/// Where the packs are: `GRAV_PACKS`, or assets/packs.
fn packs_dir() -> String {
    std::env::var("GRAV_PACKS").unwrap_or_else(|_| "assets/packs".into())
}

/// The hero's clips: standing, his three paces, and the air.
const CLIPS: [&str; 5] = ["idle", "walk", "jog", "sprint", "jump"];

/// The same dwarf every run (the seed picks face, beard and clothes).
const SEED: u32 = 11;

/// Spawn the dwarf (hidden until the hero places it), playing "idle".
/// Returns the body. `None` when the packs are
/// missing or fail to load (said on stderr).
pub fn spawn(world: &mut World) -> Option<EntityId> {
    let dir = packs_dir();
    if !Path::new(&dir).join("human/manifest.json").exists() {
        eprintln!(
            "no Actors packs in {dir}: the hero is a block figure (python tools/get_packs.py fetches them)"
        );
        return None;
    }
    match build(world, &dir) {
        Ok(built) => Some(built),
        Err(e) => {
            eprintln!("the Actors dwarf failed ({e}): the hero is a block figure");
            None
        }
    }
}

fn build(world: &mut World, dir: &str) -> Result<EntityId, String> {
    let mut builder = CharacterBuilder::load(dir).map_err(|e| e.to_string())?;
    if let Some(allow) = AllowOptions::preset("fantasy") {
        builder.allow(&allow);
    }
    let options = RandomOptions {
        race: Some(RaceChoiceOption::Race(RaceId::Dwarf)),
        gender: Some(Gender::Male),
        outfit: Some(true),
        ..Default::default()
    };
    let config = builder.random(SEED, &options);
    let build = BuildOptions {
        clips: CLIPS.iter().map(|c| c.to_string()).collect(),
        ..Default::default()
    };
    let character = builder.build(&config, &build).map_err(|e| e.to_string())?;
    let id = world
        .spawn_animated(character.model, Place::new(0.0, -9999.0, 0.0))
        .map_err(|e| e.to_string())?;
    world
        .play_animation(id, "idle")
        .map_err(|e| e.to_string())?;
    Ok(id)
}
