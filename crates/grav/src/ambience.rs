//! The sound of the places: a bed of cavern air everywhere, and loops
//! where things are: the umbral towers' glassy hum, the dwarven lane's
//! work and murmur, the goblin camp's creaks and rattles, the spring, the
//! braziers' fire. A loop plays only while the hero is within its reach,
//! and only the nearest few at once, so the mixer never fills up.

use crate::settle::{self, Glow};
use crate::space;
use engine::prelude::*;

/// The loops, by what makes them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Umbral,
    Dwarven,
    Goblin,
    Spring,
    Fire,
}

const FILES: [(Kind, &str); 5] = [
    (Kind::Umbral, "amb_umbral"),
    (Kind::Dwarven, "amb_dwarven"),
    (Kind::Goblin, "amb_goblin"),
    (Kind::Spring, "amb_spring"),
    (Kind::Fire, "amb_fire"),
];

/// At most this many place loops play at once (the mixer has 96 voices,
/// and loops are never taken away from it).
const AT_ONCE: usize = 12;
/// A loop starts within its range and stops this much further out.
const SLACK: f32 = 15.0;

/// One place that sounds.
struct Emitter {
    at: Vec3,
    kind: Kind,
    /// Full loudness within `near`, silent beyond `range`.
    near: f32,
    range: f32,
    volume: f32,
    pitch: f32,
    playing: Option<SoundId>,
}

pub struct Ambience {
    sounds: Vec<(Kind, Sound)>,
    emitters: Vec<Emitter>,
    bed: Option<SoundId>,
    /// Place loops playing now (for the report).
    pub playing: usize,
}

/// A number from 0 to 1 for the `n`th emitter, the same every run.
fn vary(n: usize) -> f32 {
    let x = ((n as u32).wrapping_mul(2_654_435_761) >> 8) as f32;
    (x / (1u32 << 24) as f32).fract()
}

/// Where the places sound, from the settlements' lights and the spring.
fn emitters() -> Vec<Emitter> {
    let mut out = Vec::new();
    let mut push = |at: Vec3, kind: Kind, near: f32, range: f32, volume: f32| {
        let n = out.len();
        out.push(Emitter {
            at,
            kind,
            near,
            range,
            volume,
            pitch: 0.94 + 0.12 * vary(n),
            playing: None,
        });
    };
    let mut dwarven = 0;
    for &(at, _, glow) in settle::lights() {
        match glow {
            Glow::Crown => push(at, Kind::Umbral, 5.0, 40.0, 0.45),
            Glow::Dwarven => {
                push(at, Kind::Fire, 1.5, 13.0, 0.6);
                // The lane's work and murmur, from every third brazier.
                if dwarven % 3 == 0 {
                    push(at, Kind::Dwarven, 10.0, 60.0, 0.55);
                }
                dwarven += 1;
            }
            Glow::Goblin => push(at, Kind::Goblin, 3.0, 30.0, 0.6),
            Glow::Umbral => {}
        }
    }
    let (spring, rot) = space::spire_frame(crate::rock::SPRING_AT.0, crate::rock::SPRING_AT.1);
    push(spring + rot * Vec3::Y * 0.3, Kind::Spring, 2.0, 22.0, 0.7);
    out
}

impl Ambience {
    /// Load the loops (one that is missing stays silent) and start the
    /// cavern's bed. Call once the settlements are planned.
    pub fn new(world: &mut World) -> Self {
        let sounds = FILES
            .iter()
            .filter_map(|&(kind, file)| {
                Sound::load(format!("assets/sounds/{file}.ogg"))
                    .ok()
                    .map(|s| (kind, s))
            })
            .collect();
        let bed = Sound::load("assets/sounds/amb_cavern.ogg")
            .ok()
            .map(|s| world.play_sound(&s, PlaySound::ambient().looped().volume(0.5).fade_in(3.0)));
        Self {
            sounds,
            emitters: emitters(),
            bed,
            playing: 0,
        }
    }

    /// Start the loops the hero has come near (the nearest `AT_ONCE`) and
    /// stop the ones left behind.
    pub fn update(&mut self, world: &mut World, listener: Vec3) {
        let mut near: Vec<(f32, usize)> = self
            .emitters
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let d = e.at.distance(listener);
                let reach = e.range + if e.playing.is_some() { SLACK } else { 0.0 };
                (d < reach).then_some((d / e.range, i))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(AT_ONCE);
        let mut wanted = vec![false; self.emitters.len()];
        for &(_, i) in &near {
            wanted[i] = true;
        }
        self.playing = 0;
        for (e, want) in self.emitters.iter_mut().zip(wanted) {
            match (e.playing, want) {
                (Some(id), false) => {
                    world.stop_sound(id, 1.5);
                    e.playing = None;
                }
                (None, true) => {
                    if let Some((_, sound)) = self.sounds.iter().find(|(k, _)| *k == e.kind) {
                        e.playing = Some(
                            world.play_sound(
                                sound,
                                PlaySound::at(e.at)
                                    .looped()
                                    .range(e.near, e.range)
                                    .volume(e.volume)
                                    .pitch(e.pitch)
                                    .fade_in(1.5),
                            ),
                        );
                    }
                }
                _ => {}
            }
            self.playing += usize::from(e.playing.is_some());
        }
    }

    /// Whether the cavern's bed is playing.
    pub fn has_bed(&self) -> bool {
        self.bed.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "full"]
    fn every_ambience_loop_is_there_and_long_enough_to_loop() {
        for (_, file) in FILES.iter().chain([&(Kind::Spring, "amb_cavern")]) {
            let path = format!(
                "{}/../../assets/sounds/{file}.ogg",
                env!("CARGO_MANIFEST_DIR")
            );
            let sound = Sound::load(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert!(sound.duration() > 8.0, "{file} is {} s", sound.duration());
        }
    }
}
