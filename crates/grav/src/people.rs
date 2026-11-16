//! The people of the towns (F5): umbrals on the road along the spire,
//! dwarves in the lane on the wall, goblins by the ring road.
//!
//! They are Actors characters, a few of each people, made on a worker
//! thread while the game already runs and drawn as Engine crowds: full
//! characters near the eye, light ones far off. Each walks its road, stops
//! to talk or stand, and runs from an open stone. Whoever a stone's pull
//! takes hold of all the same is a body like any other (movable.rs): they
//! lose their footing, physics moves them as it moves goods and rubble,
//! and they walk home when it lets go.

use crate::flora::hash;
use crate::movable::{self, Movable};
use crate::rock::{self, Rock};
use crate::space;
use actors_engine::{BuildOptions, CharacterBuilder};
use actors_generator::{AllowOptions, Gender, RaceChoiceOption, RandomOptions};
use actors_human::RaceId;
use engine::anim::AnimatedModel;
use engine::prelude::*;
use grav_core::{BodyQuery, Field, Movement};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

/// The three peoples of the test space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Folk {
    Umbral,
    Dwarf,
    Goblin,
}

const FOLKS: [Folk; 3] = [Folk::Umbral, Folk::Dwarf, Folk::Goblin];
/// How many different characters there are of each people.
const FACES: u32 = 4;

/// A stretch of road people walk: along it by `s` metres, across by `off`.
#[derive(Clone, Copy, Debug)]
enum Road {
    /// Along the top of the spire, `s` is z.
    Spire,
    /// The lane on the wall at this end (-1 or 1), `s` is |x|.
    Lane(f32),
    /// The ring road around this angle (radians), `s` metres along it.
    Ring(f32),
}

impl Road {
    fn folk(self) -> Folk {
        match self {
            Road::Spire => Folk::Umbral,
            Road::Lane(_) => Folk::Dwarf,
            Road::Ring(_) => Folk::Goblin,
        }
    }

    /// From where to where `s` runs, and how far to each side people walk.
    fn span(self) -> (f32, f32, f32) {
        match self {
            Road::Spire => (-100.0, 52.0, 3.4),
            Road::Lane(_) => (173.0, 228.0, 3.8),
            Road::Ring(_) => (-19.0, 19.0, 2.6),
        }
    }

    /// The ground there (none where the road has no ground).
    fn at(self, rock: &Rock, s: f32, off: f32) -> Option<Vec3> {
        match self {
            Road::Spire => Some(rock.spire_point(off / space::SPIRE_RADIUS, s)),
            Road::Lane(end) => Some(rock.wall_point(end * s, off)),
            Road::Ring(angle) => rock.flare_at(angle + s / rock::RING_ROAD, rock::RING_ROAD + off),
        }
    }
}

/// How many people walk a road.
fn crowd_size(road: Road) -> u32 {
    match road {
        Road::Spire => 34,
        Road::Lane(_) => 15,
        Road::Ring(_) => 11,
    }
}

const WALK: f32 = 1.25;
const RUN: f32 = 3.3;
/// People run when an open stone's reach comes this near.
const FEAR: f32 = 9.0;

struct Person {
    /// Walking speed, m/s: what this crowd's walk clip carries at an easy step.
    pace: f32,
    crowd: CrowdId,
    agent: CrowdAgentId,
    road: Road,
    s: f32,
    off: f32,
    goal: f32,
    /// Seconds still to stand here.
    wait: f32,
    /// Which way along the road they last faced (1 or -1).
    heading: f32,
    key: u32,
    /// Counts their choices, so each is another.
    turn: u32,
    pose: Option<&'static str>,
    /// Where they stand on their road.
    at: Vec3,
    /// Off their feet or off their road: where they are and how they move.
    loose: Option<Loose>,
}

/// Someone a pull has taken off the road.
struct Loose {
    /// Their body in the physics while the pull has them (none: on their
    /// feet again, walking home).
    body: Option<EntityId>,
    at: Vec3,
    /// Seconds they have lain where they came down, the pull gone.
    down: f32,
}

/// They get up this long after it has let go.
const GETS_UP: f32 = 2.5;
/// A person as a body: a capsule this tall from feet to middle, this wide.
const MIDDLE: f32 = 0.85;
const GIRTH: f32 = 0.3;
/// Further from home than this they are gone.
const LOST: f32 = 450.0;

/// One character made: which people, which of its faces.
type Made = (Folk, u32, AnimatedModel);

pub struct People {
    made: Option<Receiver<Made>>,
    persons: Vec<Person>,
    roads: Vec<Road>,
    /// The hero's movement numbers: people stand on their feet as he does.
    movement: Movement,
    /// How many are running from a stone this frame.
    pub fleeing: usize,
    /// How many a pull has off their feet or off their road this frame.
    pub taken: usize,
    /// How many a pull has carried away for good.
    pub lost: usize,
    /// How many a pull has ever taken off their feet (challenges count
    /// them as harmed, F6).
    pub swept: usize,
    unseen: CustomMaterialId,
}

impl Movable for People {
    fn count(&self) -> usize {
        self.persons.len()
    }

    fn holds_at(&self, i: usize) -> Option<Vec3> {
        let p = &self.persons[i];
        p.loose.is_none().then_some(p.at)
    }

    /// People stand on their legs as the hero does (grav-core's footing
    /// rule with his numbers).
    fn grip(&self, pull: Vec3) -> f32 {
        grav_core::grip(&self.movement, pull)
    }

    /// Off their feet: a body for physics to move, which the person is
    /// drawn at.
    fn let_go(&mut self, world: &mut World, i: usize) -> EngineResult<()> {
        let p = &mut self.persons[i];
        let body = world.spawn(Shape::ball((0.0, 0.0, 0.0), GIRTH, rgb(0, 0, 0))?);
        let place = world.crowd_agent_place(p.crowd, p.agent)?;
        let up = place
            .to_matrix()
            .transform_vector3(Vec3::Y)
            .normalize_or(Vec3::Y);
        let middle = p.at + up * MIDDLE;
        world.set_place(body, Place::new(middle.x, middle.y, middle.z))?;
        world.set_custom_material(body, self.unseen)?;
        world.set_casts_shadow(body, false)?;
        world.add_body(
            body,
            Body::dynamic()
                .shape(BodyShape::Capsule {
                    half_height: MIDDLE - GIRTH,
                    radius: GIRTH,
                })
                .density(1.0)
                .ccd(true),
        )?;
        self.swept += 1;
        p.loose = Some(Loose {
            body: Some(body),
            at: p.at,
            down: 0.0,
        });
        Ok(())
    }
}

/// The clips a townsman has: standing, walking, running and two ways of
/// standing about.
const CLIPS: [&str; 6] = ["idle", "walk", "jog", "talk", "arms folded", "jump"];

fn make(dir: String, send: std::sync::mpsc::Sender<Made>) -> Result<(), String> {
    let mut builder = CharacterBuilder::load(&dir).map_err(|e| e.to_string())?;
    if let Some(allow) = AllowOptions::preset("fantasy") {
        builder.allow(&allow);
    }
    // One of each people first, so every street has someone soon.
    for face in 0..FACES {
        for (f, folk) in FOLKS.into_iter().enumerate() {
            let race = match folk {
                Folk::Umbral => RaceId::Umbral,
                Folk::Dwarf => RaceId::Dwarf,
                Folk::Goblin => RaceId::Goblin,
            };
            let options = RandomOptions {
                race: Some(RaceChoiceOption::Race(race)),
                gender: Some(if face % 2 == 0 {
                    Gender::Female
                } else {
                    Gender::Male
                }),
                outfit: Some(true),
                ..Default::default()
            };
            let config = builder.random(4100 + f as u32 * 97 + face * 13, &options);
            let build = BuildOptions {
                clips: CLIPS.iter().map(|c| c.to_string()).collect(),
                ..Default::default()
            };
            let character = builder.build(&config, &build).map_err(|e| e.to_string())?;
            if send.send((folk, face, character.model)).is_err() {
                return Ok(());
            }
        }
    }
    Ok(())
}

impl People {
    /// Start making the people (they come into the world as they are done).
    pub fn new(unseen: CustomMaterialId, movement: Movement) -> Self {
        let roads = vec![
            Road::Spire,
            Road::Lane(-1.0),
            Road::Lane(1.0),
            Road::Ring(50.0_f32.to_radians()),
            Road::Ring(228.0_f32.to_radians()),
        ];
        let dir = std::env::var("GRAV_PACKS").unwrap_or_else(|_| "assets/packs".into());
        let made = if std::env::var("GRAV_NO_PEOPLE").is_ok() {
            None
        } else if Path::new(&dir).join("human/manifest.json").exists() {
            let (send, receive) = channel();
            std::thread::spawn(move || {
                if let Err(e) = make(dir, send) {
                    eprintln!("the townsfolk could not be made ({e})");
                }
            });
            Some(receive)
        } else {
            eprintln!("no Actors packs in {dir}: the streets are empty");
            None
        };
        Self {
            made,
            persons: Vec::new(),
            roads,
            movement,
            fleeing: 0,
            taken: 0,
            lost: 0,
            swept: 0,
            unseen,
        }
    }

    /// A character has been made: it joins every road of its people, as
    /// its share of those who walk there.
    fn arrive(&mut self, world: &mut World, made: Made) -> EngineResult<()> {
        let (folk, face, mut model) = made;
        // A part with nothing to draw (some outfits make one) has no place
        // in the model: Engine's shadow pass stops on its empty buffer.
        if model.morph_targets.is_empty() {
            model.meshes.retain(|m| !m.indices.is_empty());
        }
        if std::env::var("GRAV_PEOPLE_DEBUG").is_ok() {
            let sizes: Vec<(usize, usize)> = model
                .meshes
                .iter()
                .map(|m| (m.positions.len(), m.indices.len()))
                .collect();
            let stride = model.ground_speed("walk");
            eprintln!("{folk:?} {face}: walk {stride:?} m/s, {sizes:?}");
        }
        // The walk plays at the speed its feet carry it over the ground
        // (measured from the planted foot), so the feet do not skate.
        let stride = model
            .ground_speed("walk")
            .filter(|v| *v > 0.1)
            .unwrap_or(WALK);
        // Short strides walk a little slower, at a quicker step.
        let pace = (stride * 1.3).clamp(0.6, WALK);
        let profile = CrowdProfile::new("idle", "walk")
            .walk_speed(stride)
            .poses(["jog", "talk", "arms folded", "jump"])
            .far_detail(0.12)
            .near(16.0, 6)
            .far(420.0)
            .shadows_within(45.0);
        let crowd = world.add_crowd(Arc::new(model), profile)?;
        let rock = rock::shared();
        for (r, &road) in self.roads.iter().enumerate() {
            if road.folk() != folk {
                continue;
            }
            let (from, to, wide) = road.span();
            for n in (0..crowd_size(road)).filter(|n| n % FACES == face) {
                let key = 7000 + r as u32 * 500 + n;
                let s = from + (to - from) * hash(key, 1);
                let off = wide * (hash(key, 2) * 2.0 - 1.0);
                let Some(at) = road.at(rock, s, off) else {
                    continue;
                };
                let agent = world.add_crowd_agent(crowd, Place::new(at.x, at.y, at.z))?;
                self.persons.push(Person {
                    crowd,
                    agent,
                    road,
                    s,
                    off,
                    goal: s,
                    wait: 6.0 * hash(key, 3),
                    heading: if hash(key, 4) < 0.5 { -1.0 } else { 1.0 },
                    key,
                    pace,
                    turn: 0,
                    pose: None,
                    at,
                    loose: None,
                });
            }
        }
        Ok(())
    }

    /// Each frame: `field` is the pull now, `base` the world's own.
    pub fn update(
        &mut self,
        world: &mut World,
        field: &Field,
        base: &Field,
        dt: f32,
    ) -> EngineResult<()> {
        // At most one new character a frame (taking one in costs a moment).
        if let Some(made) = self.made.as_ref().and_then(|m| m.try_recv().ok()) {
            self.arrive(world, made)?;
        }
        let rock = rock::shared();
        self.fleeing = 0;
        self.taken = 0;
        let mut gone = Vec::new();
        let movement = self.movement;
        for (index, p) in self.persons.iter_mut().enumerate() {
            let (from, to, _) = p.road.span();
            let Some(at) = p.road.at(rock, p.s, p.off) else {
                continue;
            };
            // Off their feet or off their road.
            if let Some(l) = p.loose.as_mut() {
                self.taken += 1;
                let now = field.acceleration(&BodyQuery::at(l.at));
                let mut up = (-now).normalize_or(Vec3::Y);
                let (mut pose, mut speed, mut front) = (Some("jump"), 0.0, Vec3::Z);
                if let Some(body) = l.body {
                    // Physics has them, as it has goods and rubble.
                    let middle = world.world_transform(body)?.w_axis.truncate();
                    l.at = middle - up * MIDDLE;
                    let slow = world.body_velocity(body).is_none_or(|v| v.length() < 1.0);
                    let held =
                        movable::held(field, base, l.at, |pull| grav_core::grip(&movement, pull));
                    l.down = if slow && !held { l.down + dt } else { 0.0 };
                    if slow {
                        pose = None;
                    }
                    if l.at.distance(at) > LOST {
                        world.despawn(body);
                        gone.push(index);
                        continue;
                    }
                    if l.down > GETS_UP {
                        world.despawn(body);
                        l.body = None;
                    }
                } else {
                    // Up again: straight home, on whatever ground there is.
                    up = (-base.acceleration(&BodyQuery::at(l.at))).normalize_or(Vec3::Y);
                    let way = at - l.at;
                    if way.length() < 0.6 {
                        p.loose = None;
                        continue;
                    }
                    front = way.normalize();
                    let next = l.at + front * p.pace * dt;
                    l.at = world
                        .cast_ray(next + up * 1.2, -up, 4.0)
                        .map_or(next, |hit| hit.point);
                    (pose, speed) = (None, p.pace);
                }
                let rot = grav_core::upright(up, front);
                world.set_crowd_agent(p.crowd, p.agent, space::place(l.at, rot), speed)?;
                if pose != p.pose {
                    world.set_crowd_agent_pose(p.crowd, p.agent, pose)?;
                    p.pose = pose;
                }
                continue;
            }
            let ahead = p
                .road
                .at(rock, p.s + 0.5, p.off)
                .map(|q| (q - at).normalize_or(Vec3::Z))
                .unwrap_or(Vec3::Z);
            // An open stone near: run the other way along the road.
            let threat = field
                .sources
                .iter()
                .filter(|s| s.opening > 0.0)
                .map(|s| (s.position, s.position.distance(at) - s.radius))
                .filter(|t| t.1 < FEAR)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let mut speed = 0.0;
            let mut pose = None;
            p.at = at;
            if let Some((stone, _)) = threat {
                self.fleeing += 1;
                p.heading = if (at - stone).dot(ahead) >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                // At the road's end there is nowhere further: they stand.
                let next = (p.s + p.heading * RUN * dt).clamp(from, to);
                if (next - p.s).abs() > 1e-4 {
                    (speed, pose) = (RUN, Some("jog"));
                }
                p.s = next;
                (p.goal, p.wait) = (p.s, 1.5);
            } else if p.wait > 0.0 {
                p.wait -= dt;
                pose = p.pose.filter(|name| *name != "jog");
                if p.wait <= 0.0 {
                    p.turn += 1;
                    p.goal = from + (to - from) * hash(p.key, 10 + p.turn);
                }
            } else {
                let left = p.goal - p.s;
                if left.abs() < p.pace * dt {
                    // There: stand a while, some talking, some arms folded.
                    p.turn += 1;
                    p.wait = 2.5 + 9.0 * hash(p.key, 10 + p.turn);
                    let how = hash(p.key, 40 + p.turn);
                    pose = if how < 0.3 {
                        Some("talk")
                    } else if how < 0.5 {
                        Some("arms folded")
                    } else {
                        None
                    };
                } else {
                    p.heading = left.signum();
                    p.s += p.heading * p.pace * dt;
                    speed = p.pace;
                }
            }
            let Some(at) = p.road.at(rock, p.s, p.off) else {
                continue;
            };
            // Upright against the world's own pull, facing along the road.
            let up = base.up_at(at, Vec3::Y);
            let rot = grav_core::upright(up, ahead * p.heading);
            world.set_crowd_agent(p.crowd, p.agent, space::place(at, rot), speed)?;
            if pose != p.pose {
                world.set_crowd_agent_pose(p.crowd, p.agent, pose)?;
                p.pose = pose;
            }
        }
        // Those carried away for good leave the streets.
        for index in gone.into_iter().rev() {
            let p = self.persons.swap_remove(index);
            world.remove_crowd_agent(p.crowd, p.agent);
            self.lost += 1;
        }
        Ok(())
    }
}
