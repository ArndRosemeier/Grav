# Sounds

Made with OpenRouter, model `bytedance-seed/seed-audio-1-0` (2026-11-16), then
trimmed, levelled to -18 LUFS and stored as mono Ogg Vorbis. To make one
again: `python3 or.py sfx "<prompt>" --model <id> --out x.mp3`.

| File | Prompt |
| --- | --- |
| strain_light_0 | The sound of a brittle porous pumice slab creaking and ticking under strain, two dry small cracks, close and dry, no music, no voice |
| strain_light_1 | The sound of thin metal lattice and brittle foam stone straining, a short dry creak followed by a small sharp tick, close, no music, no voice |
| strain_light_2 | The sound of a light stone panel shifting slightly with a dry scrape and a faint crackle of grit, close, no music, no voice |
| strain_heavy_0 | The sound of a heavy stone block grinding a little against another and settling with one deep dull knock, dry, close, no music, no voice |
| strain_heavy_1 | The sound of massive masonry under strain, a low stone groan and a single deep thud, dry, no music, no voice |
| strain_heavy_2 | The sound of a big stone block scraping a finger's width and knocking once, deep and short, with falling grit, no music, no voice |
| break_light_0 | The sound of a slab of brittle porous stone snapping loose from a wall with a sharp crack and a short clatter of light fragments, no music, no voice |
| break_light_1 | The sound of thin metal bars snapping and a light stone panel breaking off with a crack, small pieces scattering on stone, no music, no voice |
| break_light_2 | The sound of a pane of thick glass and its light stone frame cracking apart and falling in pieces on a stone floor, short, no music, no voice |
| break_heavy_0 | The sound of a huge masonry block breaking loose from a wall with a deep crack, then heavy rubble tumbling on stone, no music, no voice |
| break_heavy_1 | The sound of a heavy stone wall section splitting with a low boom and big stones crashing down, short, no music, no voice |
| break_heavy_2 | The sound of a big stone block tearing out of mortar with a deep crunch and falling with two heavy impacts, no music, no voice |

## Ambience loops

Same model, made loopable (the first two seconds crossfaded onto the end),
levelled to -20 LUFS (the cavern bed to -24), mono Ogg Vorbis. Played by
`crates/grav/src/ambience.rs`.

| File | Prompt |
| --- | --- |
| amb_cavern | The sound of a vast underground cavern: a very low steady wind moving through stone, distant faint drips echoing far away, deep and calm, continuous ambience, no music, no voice |
| amb_umbral | The sound of a soft high glassy hum and gentle crystalline chiming drifting in still air, quiet and eerie, continuous ambience, no music, no voice |
| amb_dwarven | The sound of a busy stone street in a mountain hold: distant hammer strikes on an anvil, low muffled crowd murmur without words, a heavy cart rolling on cobbles, continuous ambience, no music |
| amb_goblin | The sound of a ramshackle camp: wooden planks creaking, loose sheet metal rattling in a draught, a faint distant clatter of pots, continuous ambience, no music, no voice |
| amb_spring | The sound of a small spring bubbling up into a stone basin and trickling over its rim, close, continuous, no music, no voice |
| amb_fire | The sound of a coal fire burning in an iron brazier, soft crackling and gentle roar, close, continuous, no music, no voice |
