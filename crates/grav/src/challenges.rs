//! Challenges in the test space and the ways to play (plan stage 5).
//!
//! The rules of a challenge (winning, failing, the score) are in
//! `grav_core::challenge`; here they are set in this world: which tower to
//! reach, which shack to bring down, which crate to carry where, and what
//! the world did on the way (pieces broken, goods knocked loose, people
//! swept off their feet, stones thrown).
//!
//! Modes: the sandbox (the world, free) and the challenges one by one
//! (the single puzzle mode). M opens the menu. Best scores are saved in
//! `progress.json` next to the game.
//!
//! `GRAV_CHALLENGE=<id>` starts a challenge straight away, and `GRAV_MENU=1`
//! shows the menu in a verify run (for screenshots). `GRAV_LOAD=1` loads
//! the saved game at the start.

use crate::boxes::Boxes;
use crate::goods::Goods;
use crate::hero::Hero;
use crate::people::{People, Role};
use crate::response::Response;
use crate::settle::{self, Make};
use crate::town::Town;
use engine::egui::{self, Align2, Color32, RichText};
use engine::prelude::*;
use grav_core::challenge::{Constraints, Par, Reward};
use grav_core::{
    Attempt, Challenge, Charge, Collateral, Face, Objective, Observed, Outcome, Program,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const PROGRESS_FILE: &str = "progress.json";

/// What a challenge's names stand for in this world.
#[derive(Clone, Copy, Default)]
struct Links {
    /// The building to bring down (kind, place).
    target: Option<(usize, usize)>,
    /// Buildings that must stand.
    spare: Option<(usize, usize)>,
    /// The goods to carry.
    cargo: Option<usize>,
    /// The ground's up at the start.
    up: Vec3,
    /// A heart shard set down when the challenge starts.
    shard: Option<Vec3>,
    /// Raiders: this many goblins nearest this place.
    foes: Option<(Vec3, usize)>,
    /// Where the goods for a barricade lie.
    aim: Option<Vec3>,
}

/// The world's tallies when an attempt began.
#[derive(Clone, Copy, Default)]
struct Tally {
    broken: usize,
    goods: usize,
    swept: usize,
    thrown: u32,
}

/// The best a player has done.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Progress {
    /// Challenge id: best points and stars.
    best: BTreeMap<String, (u32, u8)>,
}

enum Mode {
    /// The menu is up (at the start, or after M).
    Menu,
    Sandbox,
    Playing {
        index: usize,
        attempt: Attempt,
        start: Tally,
    },
    /// Won or failed: the result shows until the player picks again.
    Over {
        index: usize,
        attempt: Attempt,
    },
}

pub struct Challenges {
    list: Vec<Challenge>,
    links: Vec<Links>,
    mode: Mode,
    /// The mode before the menu was opened, to go back to.
    before: Option<Mode>,
    progress: Progress,
    /// A challenge to start on the next frame (the menu's choice).
    pending: Option<usize>,
    /// The label over the goal.
    marker: Option<EntityId>,
    /// The people a challenge has given a part (their keys).
    cast: Vec<u32>,
    /// `GRAV_SOLVE=1`: each challenge plays its own solution (verify runs
    /// prove that it can be won), and the step it is at.
    solving: bool,
    solve_step: u32,
    /// The sandbox was picked: take the goal's label away.
    leave: bool,
    /// Save or load asked for in the menu, for the game to do.
    pub file: Option<FileAct>,
    /// Is there a saved game to go on with.
    pub saved: bool,
    /// What saving or loading said last.
    pub file_note: Option<String>,
}

/// Saving or loading, asked for in the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAct {
    Save,
    Load,
}

/// The ground under `p` along `-up`: walked down from above until the
/// rock begins (the physics may not know the rock yet on the first frame).
pub fn ground(world: &World, p: Vec3, up: Vec3) -> Vec3 {
    if let Some(q) = ground_point(p, up) {
        return q;
    }
    world
        .cast_ray(p + up * 4.0, -up, 12.0)
        .map_or(p, |hit| hit.point + up * 0.05)
}

/// How steep the rock is under `p`, degrees from level (None: no ground).
fn slope_deg(p: Vec3, up: Vec3) -> Option<f32> {
    let a = up.any_orthonormal_vector();
    let b = up.cross(a);
    let o = ground_point(p, up)?;
    let pa = ground_point(p + a, up)?;
    let pb = ground_point(p + b, up)?;
    let n = (pa - o).cross(pb - o).normalize_or_zero();
    Some(n.dot(up).abs().clamp(0.0, 1.0).acos().to_degrees())
}

/// The rock's surface under `p` along `-up`, from the rock's own shape.
fn ground_point(p: Vec3, up: Vec3) -> Option<Vec3> {
    let rock = crate::rock::shared();
    let solid = |q: Vec3| rock.in_spire(q) || rock.in_island(q);
    let mut q = p + up * 25.0;
    if solid(q) {
        return None;
    }
    for _ in 0..240 {
        let next = q - up * 0.25;
        if solid(next) {
            return Some(q + up * 0.1);
        }
        q = next;
    }
    None
}

/// A place `dist` from `center` on the ground plane of `up`, as far from
/// every other thing built as can be (so the hero and the camera behind
/// him stand in the open) and near a road, preferring `toward` when it is as good.
fn open_spot(center: Vec3, up: Vec3, dist: f32, toward: Vec3) -> Vec3 {
    let sites = settle::sites();
    let a = grav_core::along_ground(toward, up);
    let b = up.cross(a);
    (0..24)
        .map(|n| {
            let angle = n as f32 / 24.0 * std::f32::consts::TAU;
            center + (a * angle.cos() + b * angle.sin()) * dist
        })
        .max_by(|p, q| {
            let room = |x: &Vec3| {
                sites
                    .iter()
                    .filter(|s| s.at.distance(center) > 1.0)
                    .map(|s| s.at.distance(*x) - s.clear)
                    .fold(f32::MAX, f32::min)
                    .min(12.0)
                    // On or near a road, where the ground is even.
                    - 0.5 * crate::rock::Rock::path_distance(*x).min(30.0)
            };
            // Ties (both roomy) go to the one nearer `toward`.
            (room(p) - 0.01 * p.distance(center + a * dist))
                .total_cmp(&(room(q) - 0.01 * q.distance(center + a * dist)))
        })
        .unwrap_or(center + a * dist)
}

impl Challenges {
    /// The three challenges of the test space, made from what the
    /// settlements built (settle::sites) and found in the town and goods.
    pub fn new(town: &Town, goods: &Goods) -> Self {
        let sites = settle::sites();
        let mut list = Vec::new();
        let mut links = Vec::new();
        let of = |make: Make| sites.iter().filter(move |s| s.make == make);

        // Reach: a guild beacon hangs high over the spire road where no
        // building stands near (umbral towers are foam: a stone strong
        // enough to lift the hero would tear them apart).
        let rock = crate::rock::shared();
        let built: Vec<Vec3> = sites
            .iter()
            .filter(|s| !s.make.loose())
            .map(|s| s.at)
            .collect();
        let room = |p: Vec3| built.iter().map(|b| b.distance(p)).fold(f32::MAX, f32::min);
        let lookout = (-60..=60)
            .map(|n| rock.spire_point(0.0, n as f32 * 4.0))
            .max_by(|a, b| room(*a).total_cmp(&room(*b)));
        if let Some(foot) = lookout.filter(|p| room(*p) > 24.0) {
            let base = crate::space::base_field();
            let up = base.up_at(foot, Vec3::Y);
            let along = grav_core::along_ground(Vec3::Z, up);
            let start = foot - along * 5.0;
            list.push(Challenge {
                id: "reach_beacon".into(),
                title: "The guild beacon".into(),
                brief: "The guild's beacon hangs nine metres over the road. Get up to it. One stone alone will not lift you from the ground.".into(),
                start: start.into(),
                start_yaw_deg: 0.0,
                objective: Objective::Reach {
                    target: (foot + up * 9.0).into(),
                    radius: 2.5,
                },
                constraints: Constraints {
                    time_limit_s: Some(240.0),
                    ..Default::default()
                },
                reward: Reward {
                    coin: 20,
                    standing: vec![("umbrals".into(), 2)],
                },
                par: Par {
                    time_s: 30.0,
                    boxes: 2,
                    collateral: 0.0,
                },
            });
            links.push(Links {
                up,
                ..Default::default()
            });
        }

        // Demolish: a condemned goblin shack, while its neighbour stands.
        let shacks: Vec<_> = of(Make::Shack).collect();
        if let Some(&s) = shacks.get(2) {
            let neighbour = shacks
                .iter()
                .filter(|o| o.at.distance(s.at) > 1.0)
                .min_by(|a, b| a.at.distance(s.at).total_cmp(&b.at.distance(s.at)));
            let up = s.rot * Vec3::Y;
            let away = neighbour.map_or(s.rot * Vec3::Z, |n| (s.at - n.at).normalize_or(Vec3::Z));
            let start = open_spot(s.at, up, 16.0, away);
            let mut spare = Vec::new();
            let mut spare_link = None;
            if let Some(n) = neighbour {
                spare.push("neighbour".to_string());
                spare_link = town.building_at(n.at);
            }
            list.push(Challenge {
                id: "demolish_shack".into(),
                title: "The condemned shack".into(),
                brief: "The camp wants the rotten shack gone. Bring at least half of it down, and leave the next one standing.".into(),
                start: start.into(),
                start_yaw_deg: 0.0,
                objective: Objective::Demolish {
                    structure: "shack".into(),
                    share: 0.5,
                },
                constraints: Constraints {
                    time_limit_s: Some(240.0),
                    spare,
                    ..Default::default()
                },
                reward: Reward {
                    coin: 25,
                    standing: vec![("goblins".into(), 3)],
                },
                par: Par {
                    time_s: 75.0,
                    boxes: 2,
                    collateral: 10.0,
                },
            });
            links.push(Links {
                target: town.building_at(s.at),
                spare: spare_link,
                up,
                ..Default::default()
            });
        }

        // Haul: the crate with the most room around it and along a way
        // nine metres on, so stones that hop it there need not come near
        // a building (a stone breaks light work within about 5 m).
        // Directions along the ground around a point, the world's up
        // there, and a penalty for being off the roads (steep, rough).
        let up_at = |p: Vec3| crate::space::base_field().up_at(p, Vec3::Y);
        let around = move |p: Vec3| {
            let up = up_at(p);
            let a = up.any_orthonormal_vector();
            let b = up.cross(a);
            (0..16).map(move |n| {
                let t = n as f32 / 16.0 * std::f32::consts::TAU;
                a * t.cos() + b * t.sin()
            })
        };
        let flat = |p: Vec3| 0.2 * crate::rock::Rock::path_distance(p).min(20.0);
        let edge = |p: Vec3| {
            sites
                .iter()
                .filter(|s| !s.make.loose())
                .map(|s| s.at.distance(p) - s.clear)
                .fold(f32::MAX, f32::min)
        };
        let way = of(Make::Crate)
            .flat_map(|c| around(c.at).map(move |dir| (c, dir)))
            .filter_map(|(c, dir)| {
                // Where the two stones hang must be clear; the end only
                // needs to be off the buildings, on ground.
                let stones = edge(c.at + dir * 3.5).min(edge(c.at + dir * 7.0));
                let end = c.at + dir * 9.0;
                let to = ground_point(end, up_at(end)).filter(|_| edge(end) > 1.0)?;
                Some((c, to, stones - flat(c.at + dir * 4.5)))
            })
            .filter(|w| w.2 > 3.0)
            .max_by(|x, y| x.2.total_cmp(&y.2));
        if let Some((c, to, _)) = way {
            let up = up_at(c.at);
            {
                let start = open_spot(c.at, up, 6.0, c.at - to);
                list.push(Challenge {
                    id: "haul_crate".into(),
                    title: "A crate down the road".into(),
                    brief: "Bring the crate nine metres to the marked place. It is too heavy to carry, and no stone pulls that far: move it in hops.".into(),
                    start: start.into(),
                    start_yaw_deg: 0.0,
                    objective: Objective::Haul {
                        cargo: "crate".into(),
                        to: to.into(),
                        radius: 3.0,
                    },
                    constraints: Constraints {
                        time_limit_s: Some(300.0),
                        max_collateral: Some(80.0),
                        ..Default::default()
                    },
                    reward: Reward {
                        coin: 15,
                        standing: vec![("goblins".into(), 2)],
                    },
                    par: Par {
                        time_s: 90.0,
                        boxes: 2,
                        collateral: 4.0,
                    },
                });
                links.push(Links {
                    cargo: goods.item_at(c.at),
                    up,
                    ..Default::default()
                });
            }
        }

        // Hold: a heart shard has come down by the dwarven stores; keep
        // the guild's cask where it stands while it pushes.
        if let Some(c) = of(Make::Cask).nth(3) {
            let up = c.rot * Vec3::Y;
            // In front of the cask, away from the hold behind it; the hero
            // comes from the side.
            let shard = c.at + grav_core::along_ground(c.rot * Vec3::Z, up) * 2.5;
            let start = open_spot(c.at, up, 10.0, c.rot * Vec3::X);
            list.push(Challenge {
                id: "hold_cask".into(),
                title: "The guild's cask".into(),
                brief: "A splinter of the heart has struck by the stores. As its charge wakes it will push everything away: keep the marked cask in its place for 40 seconds.".into(),
                start: start.into(),
                start_yaw_deg: 0.0,
                objective: Objective::Hold {
                    thing: "cask".into(),
                    anchor: c.at.into(),
                    radius: 2.0,
                    seconds: 40.0,
                },
                constraints: Constraints {
                    time_limit_s: Some(70.0),
                    ..Default::default()
                },
                reward: Reward {
                    coin: 20,
                    standing: vec![("dwarves".into(), 3)],
                },
                par: Par {
                    time_s: 40.0,
                    boxes: 1,
                    collateral: 6.0,
                },
            });
            links.push(Links {
                cargo: goods.item_at(c.at),
                shard: Some(shard),
                up,
                ..Default::default()
            });
        }

        // Build: a barricade from goods lying about. The zone is beside a
        // heap of them (at least four within 4 m, which a stone there
        // draws in), with the most room from buildings, on even ground,
        // so the stone that gathers them spares the houses.
        let loose: Vec<Vec3> = sites
            .iter()
            .filter(|g| g.make.loose())
            .map(|g| g.at)
            .collect();
        let stocked = loose
            .iter()
            .filter_map(|&g| {
                // A heap: goods within 3 m of this one, and their middle.
                let heap: Vec<Vec3> = loose
                    .iter()
                    .copied()
                    .filter(|q| q.distance(g) < 3.0)
                    .collect();
                if heap.len() < 4 {
                    return None;
                }
                let middle = heap.iter().copied().sum::<Vec3>() / heap.len() as f32;
                // Out from the nearest house, 2.5 m past the heap.
                let house = sites
                    .iter()
                    .filter(|s| !s.make.loose())
                    .min_by(|a, b| a.at.distance(middle).total_cmp(&b.at.distance(middle)))?;
                let out = grav_core::along_ground(middle - house.at, up_at(middle));
                let zone = middle + out * 2.5;
                slope_deg(zone, up_at(zone)).filter(|a| *a < 25.0)?;
                Some((middle, zone, out, edge(zone) - flat(zone)))
            })
            .max_by(|x, y| x.3.total_cmp(&y.3));
        if let Some((middle, zone, out, _)) = stocked {
            let up = up_at(zone);
            let zone = ground_point(zone, up).unwrap_or(zone);
            // The hero waits on even ground nine metres out.
            let start = around(zone)
                .map(|d| zone + d * 9.0)
                .filter(|p| slope_deg(*p, up).is_some_and(|a| a < 25.0))
                .max_by(|p, q| {
                    (edge(*p) - 0.3 * p.distance(zone + out * 9.0))
                        .total_cmp(&(edge(*q) - 0.3 * q.distance(zone + out * 9.0)))
                })
                .and_then(|p| ground_point(p, up))
                .unwrap_or_else(|| open_spot(zone, up, 9.0, out));
            list.push(Challenge {
                id: "build_barricade".into(),
                title: "A barricade".into(),
                brief: "Raiders are coming. Pile three loose things (goods, planks, rubble) in the marked place to block the way.".into(),
                start: start.into(),
                start_yaw_deg: 0.0,
                objective: Objective::Build {
                    zone: zone.into(),
                    radius: 2.0,
                    count: 3,
                },
                constraints: Constraints {
                    time_limit_s: Some(240.0),
                    no_harm: true,
                    ..Default::default()
                },
                reward: Reward {
                    coin: 25,
                    standing: vec![("goblins".into(), 3)],
                },
                par: Par {
                    time_s: 40.0,
                    boxes: 1,
                    collateral: 20.0,
                },
            });
            links.push(Links {
                up,
                aim: Some(middle),
                ..Default::default()
            });
        }

        // Fight: at the goblin camp on the far side of the ring road.
        // `ring(s, off)` is a point on the road, `s` m along it.
        let camp = 228.0_f32.to_radians();
        let ring = |s: f32, off: f32| {
            rock.flare_at(
                camp + s / crate::rock::RING_ROAD,
                crate::rock::RING_ROAD + off,
            )
        };
        // The hero waits on the road where the raiders reach him clear of
        // the shacks, so a stone among them does not wreck the camp too.
        let wait = (25..=44)
            .flat_map(|d| [d as f32, -(d as f32)])
            .filter_map(|d| {
                let here = ring(d, 0.0)?;
                let met = ring(d - d.signum() * 8.0, 0.0)?;
                Some((here, edge(here).min(edge(met))))
            })
            .max_by(|x, y| x.1.total_cmp(&y.1))
            .map(|w| w.0);
        if let (Some(camp_mid), Some(start)) = (ring(0.0, 0.0), wait) {
            let base = crate::space::base_field();
            let up = base.up_at(start, Vec3::Z);
            list.push(Challenge {
                id: "fight_raiders".into(),
                title: "Raiders".into(),
                brief: "Three raiders from the camp are coming for you. Knock all three off their feet before they beat you down.".into(),
                start: start.into(),
                start_yaw_deg: 0.0,
                objective: Objective::Fight { foes: 3 },
                constraints: Constraints {
                    time_limit_s: Some(180.0),
                    ..Default::default()
                },
                reward: Reward {
                    coin: 30,
                    standing: vec![("goblins".into(), -2)],
                },
                par: Par {
                    time_s: 45.0,
                    boxes: 2,
                    collateral: 10.0,
                },
            });
            links.push(Links {
                up,
                foes: Some((camp_mid, 3)),
                ..Default::default()
            });
        }

        let progress = std::fs::read_to_string(PROGRESS_FILE)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let wanted = std::env::var("GRAV_CHALLENGE").ok();
        let pending = wanted.and_then(|id| list.iter().position(|c| c.id == id));
        // The menu at the start, unless a run for screenshots or checks.
        let quiet = ["GRAV_TOUR", "ENGINE_VERIFY", "GRAV_MEASURE"]
            .iter()
            .any(|v| std::env::var(v).is_ok());
        let quiet = quiet && std::env::var("GRAV_MENU").is_err();
        let mode = if quiet || pending.is_some() {
            Mode::Sandbox
        } else {
            Mode::Menu
        };
        Self {
            list,
            links,
            mode,
            before: None,
            progress,
            pending,
            marker: None,
            cast: Vec::new(),
            solving: std::env::var("GRAV_SOLVE").is_ok(),
            solve_step: 0,
            leave: false,
            file: std::env::var("GRAV_LOAD").is_ok().then_some(FileAct::Load),
            saved: crate::save::SaveGame::exists(),
            file_note: None,
        }
    }

    pub fn count(&self) -> usize {
        self.list.len()
    }

    /// Is the menu up (the game keeps clicks away from throwing then).
    pub fn menu_open(&self) -> bool {
        matches!(self.mode, Mode::Menu)
    }

    fn tally(boxes: &Boxes, response: &Response, goods: &Goods, people: &People) -> Tally {
        Tally {
            broken: response.broken_total,
            goods: goods.loose_total,
            swept: people.swept,
            thrown: boxes.thrown,
        }
    }

    /// Each frame: start what was picked, watch the world, judge.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut World,
        input: &Input,
        hero: &mut Hero,
        boxes: &Boxes,
        town: &Town,
        response: &Response,
        goods: &Goods,
        people: &mut People,
        shards: &mut crate::shards::Shards,
        dt: f32,
    ) -> EngineResult<()> {
        if input.pressed(Key::M) {
            if self.menu_open() {
                self.mode = self.before.take().unwrap_or(Mode::Sandbox);
            } else {
                let was = std::mem::replace(&mut self.mode, Mode::Menu);
                self.before = Some(was);
            }
        }
        if std::mem::take(&mut self.leave) {
            self.mark(world, None)?;
        }
        if let Some(index) = self.pending.take() {
            let c = &self.list[index];
            let up = self.links[index].up;
            let feet = ground(world, c.start(), up);
            let toward = match (&c.objective, self.links[index].target) {
                (Objective::Demolish { .. }, Some((k, i))) => town.place_of(k, i),
                _ => c.goal(),
            }
            .unwrap_or(feet + up.any_orthonormal_vector());
            self.mark(world, c.goal())?;
            shards.clear(world);
            people.release();
            self.cast.clear();
            self.solve_step = 0;
            if let Some(at) = self.links[index].shard {
                shards.place(world, ground(world, at, up), up, 20.0)?;
            }
            hero.put(world, feet, up, toward - feet)?;
            self.mode = Mode::Playing {
                index,
                attempt: Attempt::new(),
                start: Self::tally(boxes, response, goods, people),
            };
            self.before = None;
        }
        let Mode::Playing {
            index,
            attempt,
            start,
        } = &mut self.mode
        else {
            return Ok(());
        };
        let c = &self.list[*index];
        let l = self.links[*index];
        // People take their parts once they have come into the world.
        if self.cast.is_empty() {
            if let Some((at, n)) = l.foes {
                self.cast = people.cast(at, n, Role::Foe);
            }
        }
        let cast_ready = !self.cast.is_empty() || l.foes.is_none();
        let now = Self::tally(boxes, response, goods, people);
        // What a hazard (a shard) breaks before the hero has thrown
        // anything is not the hero's doing.
        if now.thrown == start.thrown {
            *start = now;
        }
        let (target_total, target_broken) =
            l.target.map_or((1, 0), |b| town.broken_in(response, b));
        let spared_broken = l.spare.map_or(0, |b| town.broken_in(response, b).1);
        let cargo_loose = l.cargo.is_some_and(|i| goods.is_loose(i));
        let pieces = (now.broken - start.broken).saturating_sub(target_broken);
        let seen = Observed {
            hero: hero.feet(world),
            cargo: l.cargo.and_then(|i| goods.where_is(world, i)),
            target_down: target_broken as f32 / target_total.max(1) as f32,
            spared_broken: spared_broken as u32,
            collateral: Collateral {
                pieces: pieces as u32,
                goods: (now.goods - start.goods).saturating_sub(cargo_loose as usize) as u32,
                // Raiders are meant to be swept (each may be a few times).
                people: (now.swept - start.swept).saturating_sub(self.cast.len() * 4) as u32,
            },
            boxes: now.thrown - start.thrown,
            lost: match c.objective {
                Objective::Hold { .. } => {
                    l.cargo.is_some_and(|i| goods.where_is(world, i).is_none())
                }
                _ => false,
            },
            in_zone: match &c.objective {
                Objective::Build { zone, radius, .. } => {
                    let zone = Vec3::from(*zone);
                    let near = |p: &Vec3| p.distance(zone) <= *radius;
                    (goods
                        .resting_loose(world)
                        .iter()
                        .filter(|p| near(p))
                        .count()
                        + response
                            .resting_debris(world)
                            .iter()
                            .filter(|p| near(p))
                            .count()) as u32
                }
                _ => 0,
            },
            foes_down: if l.foes.is_some() {
                people.beaten(&self.cast) as u32
            } else {
                0
            },
            hero_down: hero.health <= 0.0,
        };
        if !cast_ready {
            // Waiting for the people to come into the world: the clock
            // has not started.
            return Ok(());
        }
        attempt.step(c, dt, &seen);
        let linked = match c.objective {
            Objective::Reach { .. } => true,
            Objective::Demolish { .. } => {
                l.target.is_some() && (c.constraints.spare.is_empty() || l.spare.is_some())
            }
            Objective::Haul { .. } | Objective::Hold { .. } => l.cargo.is_some(),
            Objective::Build { .. } => true,
            Objective::Rescue { .. } | Objective::Fight { .. } => cast_ready,
        };
        world.check("the challenge found what it is about", linked);
        if (1.5..1.7).contains(&attempt.elapsed_s) {
            world.check(
                "the hero can walk where the challenge starts",
                hero.walking_allowed,
            );
        }
        world.report("challenge_elapsed", attempt.elapsed_s);
        world.report(
            "challenge_outcome",
            match &attempt.outcome {
                Outcome::Running => "running".to_string(),
                Outcome::Won => "won".to_string(),
                Outcome::Failed(why) => format!("failed: {why}"),
            },
        );
        world.report("challenge_stones", attempt.boxes);
        world.report("challenge_damage", attempt.collateral.points());
        world.report("challenge_target_down", seen.target_down);
        world.report("challenge_in_zone", seen.in_zone);
        world.report("challenge_foes_down", seen.foes_down);
        if let (Some(first), Some(last)) = (response.broken_at.first(), response.broken_at.last()) {
            world.report("broken_first_last", [first.to_array(), last.to_array()]);
        }
        if let Some(goal) = c.goal() {
            world.report("challenge_goal", goal.to_array());
        }
        world.report(
            "challenge_collateral",
            [
                attempt.collateral.pieces,
                attempt.collateral.goods,
                attempt.collateral.people,
            ],
        );
        if let Some(cargo) = seen.cargo {
            world.report(
                "challenge_cargo_to_go_m",
                c.goal().map_or(0.0, |g| g.distance(cargo)),
            );
        }
        if attempt.outcome != Outcome::Running {
            let attempt = attempt.clone();
            let index = *index;
            if let Some(score) = attempt.score(c) {
                let best = self.progress.best.entry(c.id.clone()).or_insert((0, 0));
                if score.points > best.0 {
                    *best = (score.points, score.stars);
                    self.save();
                }
            }
            self.mode = Mode::Over { index, attempt };
            self.mark(world, None)?;
            people.release();
        }
        Ok(())
    }

    /// A name over the place to get to, while a challenge runs there.
    fn mark(&mut self, world: &mut World, goal: Option<Vec3>) -> EngineResult<()> {
        if let Some(old) = self.marker.take() {
            world.despawn(old);
        }
        if let Some(p) = goal {
            let id = world.spawn_pivot(Place::new(p.x, p.y, p.z));
            world.set_label(id, Label::new("Here").size(18.0).color(rgb(250, 205, 90)))?;
            self.marker = Some(id);
        }
        Ok(())
    }

    /// A challenge's own solution (`GRAV_SOLVE=1`): stones set where a
    /// throw from the start can put them, with the programs that win.
    /// Every challenge in the menu has one, and a verify run of it must
    /// end in a win, so none ships that cannot be solved.
    #[allow(clippy::too_many_arguments)]
    pub fn solve(
        &mut self,
        world: &mut World,
        boxes: &mut Boxes,
        town: &Town,
        goods: &Goods,
        people: &People,
        hero_at: Vec3,
    ) -> EngineResult<()> {
        let Mode::Playing { index, attempt, .. } = &self.mode else {
            return Ok(());
        };
        if !self.solving {
            return Ok(());
        }
        let (c, l, t) = (&self.list[*index], self.links[*index], attempt.elapsed_s);
        let up = l.up;
        let all = Face::ALL;
        let attracting = boxes
            .stones
            .iter()
            .position(|s| s.spec.charge == Charge::Attracting && s.is_ready());
        let second = boxes
            .stones
            .iter()
            .enumerate()
            .filter(|(_, s)| s.spec.charge == Charge::Attracting && s.is_ready())
            .nth(1)
            .map(|(i, _)| i);
        let repelling = boxes
            .stones
            .iter()
            .position(|s| s.spec.charge == Charge::Repelling && s.is_ready());
        // A stone set at `at`, opening after `open` s, dropping after
        // `drop` s; all faces, or only the front one turned to `aim`.
        let mut set = |i: Option<usize>, at: Vec3, open: f32, drop: f32, aim: Option<Vec3>| {
            if let Some(i) = i {
                let s = &mut boxes.stones[i];
                let front = aim.map(|a| grav_core::along_ground(a - at, up));
                s.program = match front {
                    Some(_) => Program::only(&[Face::Front], open, drop),
                    None => Program::only(&all, open, drop),
                };
                let frame = grav_core::facing(up, front.unwrap_or(up.any_orthonormal_vector()));
                if s.throw(at, Vec3::ZERO, frame) {
                    s.rest_at(at);
                    boxes.thrown += 1;
                }
            }
        };
        let step = self.solve_step;
        match (c.id.as_str(), step) {
            // A repelling shard at the feet and a stone at the beacon:
            // pushed up and pulled up together.
            ("reach_beacon", 0) if t > 2.0 => {
                set(repelling, hero_at + up * 0.3, 0.0, 40.0, None);
                set(attracting, c.goal().unwrap_or(hero_at), 0.0, 40.0, None);
                self.solve_step = 1;
            }
            ("demolish_shack", 0) if t > 2.0 => {
                if let Some(at) = l.target.and_then(|(k, i)| town.place_of(k, i)) {
                    set(attracting, at + up * 1.6, 0.0, 12.0, None);
                }
                self.solve_step = 1;
            }
            // Two hops: the first stone pulls the crate 3.5 m and comes
            // down, the second opens as it does and pulls it on.
            ("haul_crate", 0) if t > 2.0 => {
                if let (Some(from), Some(to)) =
                    (l.cargo.and_then(|i| goods.where_is(world, i)), c.goal())
                {
                    let dir = grav_core::along_ground(to - from, up);
                    set(attracting, from + dir * 3.5 + up * 0.6, 0.2, 5.0, None);
                    set(second, from + dir * 7.0 + up * 0.6, 5.5, 14.0, None);
                }
                self.solve_step = 1;
            }
            // A stone on the cask holds it against the shard's push.
            ("hold_cask", 0) if t > 2.0 => {
                if let Some(at) = l.cargo.and_then(|i| goods.where_is(world, i)) {
                    set(attracting, at + up * 0.45, 0.0, 80.0, None);
                }
                self.solve_step = 1;
            }
            ("build_barricade", 0) if t > 2.0 => {
                if let Some(zone) = c.goal() {
                    // Only the face towards the goods: the house behind
                    // the stone feels nothing.
                    set(attracting, zone + up * 0.7, 0.2, 8.0, l.aim);
                }
                self.solve_step = 1;
            }
            // When the raiders are close, a stone among them, then the
            // other where the rest are.
            ("fight_raiders", n) if n < 2 => {
                let foes: Vec<Vec3> = self
                    .cast
                    .iter()
                    .filter_map(|&k| people.where_is(k))
                    .collect();
                let close: Vec<Vec3> = foes
                    .iter()
                    .copied()
                    .filter(|p| p.distance(hero_at) < 8.0)
                    .collect();
                if !close.is_empty() && attracting.is_some() {
                    let mid = close.iter().copied().sum::<Vec3>() / close.len() as f32;
                    set(attracting, mid + up * 2.5, 0.0, 5.0, None);
                    self.solve_step = n + 1;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(&self.progress) {
            if let Err(e) = std::fs::write(PROGRESS_FILE, text) {
                eprintln!("{PROGRESS_FILE}: {e}");
            }
        }
    }

    /// The menu, the challenge panel and the result.
    pub fn hud(&mut self, frame: &Frame) {
        let ctx = frame.ui.ctx();
        match &self.mode {
            Mode::Menu => self.menu(ctx),
            Mode::Sandbox => {}
            Mode::Playing { index, attempt, .. } => {
                let c = &self.list[*index];
                panel(ctx, c, attempt, None);
            }
            Mode::Over { index, attempt } => {
                let c = &self.list[*index];
                let best = self.progress.best.get(&c.id).copied();
                let (index, attempt) = (*index, attempt.clone());
                if let Some(again) = result(ctx, c, &attempt, best) {
                    if again {
                        self.pending = Some(index);
                    } else {
                        self.mode = Mode::Menu;
                    }
                }
            }
        }
    }

    fn menu(&mut self, ctx: &egui::Context) {
        let gold = Color32::from_rgb(240, 200, 130);
        egui::Window::new("Grav")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .min_width(420.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("GRAV").size(34.0).color(gold).strong());
                    ui.label(RichText::new("How do you want to play?").size(15.0));
                });
                ui.add_space(10.0);
                if ui
                    .add_sized(
                        [400.0, 34.0],
                        egui::Button::new(RichText::new("Sandbox: the world, free").size(16.0)),
                    )
                    .clicked()
                {
                    self.mode = Mode::Sandbox;
                    self.before = None;
                    self.leave = true;
                }
                ui.horizontal(|ui| {
                    if self.saved
                        && ui
                            .add_sized(
                                [196.0, 28.0],
                                egui::Button::new(RichText::new("Load the saved game").size(14.0)),
                            )
                            .clicked()
                    {
                        self.file = Some(FileAct::Load);
                        self.mode = Mode::Sandbox;
                        self.before = None;
                        self.leave = true;
                    }
                    let in_sandbox = matches!(self.before, Some(Mode::Sandbox));
                    if in_sandbox
                        && ui
                            .add_sized(
                                [196.0, 28.0],
                                egui::Button::new(RichText::new("Save this game").size(14.0)),
                            )
                            .clicked()
                    {
                        self.file = Some(FileAct::Save);
                        self.mode = Mode::Sandbox;
                        self.before = None;
                    }
                });
                if let Some(note) = &self.file_note {
                    ui.label(RichText::new(note).size(12.0).weak());
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Challenges").size(16.0).color(gold));
                for (i, c) in self.list.iter().enumerate() {
                    let stars = self
                        .progress
                        .best
                        .get(&c.id)
                        .map_or("".to_string(), |&(p, s)| {
                            format!("   {}  {p}", "★".repeat(s as usize))
                        });
                    let kind = match c.objective {
                        Objective::Reach { .. } => "Reach",
                        Objective::Demolish { .. } => "Demolish",
                        Objective::Haul { .. } => "Haul",
                        Objective::Hold { .. } => "Hold",
                        Objective::Build { .. } => "Build",
                        Objective::Rescue { .. } => "Rescue",
                        Objective::Fight { .. } => "Fight",
                    };
                    let text = format!("{kind}: {}{stars}", c.title);
                    if ui
                        .add_sized(
                            [400.0, 30.0],
                            egui::Button::new(RichText::new(text).size(15.0)),
                        )
                        .on_hover_text(&c.brief)
                        .clicked()
                    {
                        self.pending = Some(i);
                    }
                }
                ui.add_space(6.0);
                ui.label(RichText::new("M opens this menu again.").size(12.0).weak());
            });
    }
}

/// The running challenge: what to do, time, stones, damage.
fn panel(ctx: &egui::Context, c: &Challenge, a: &Attempt, _best: Option<(u32, u8)>) {
    egui::Area::new(egui::Id::new("challenge_panel"))
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 10.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_black_alpha(150))
                .corner_radius(6.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_max_width(460.0);
                    ui.label(
                        RichText::new(&c.title)
                            .size(17.0)
                            .color(Color32::from_rgb(240, 200, 130))
                            .strong(),
                    );
                    ui.label(RichText::new(&c.brief).size(13.0));
                    let time = match a.time_left(c) {
                        Some(left) => format!("{:.0} s left", left),
                        None => format!("{:.0} s", a.elapsed_s),
                    };
                    let damage = a.collateral.points();
                    ui.label(
                        RichText::new(format!(
                            "{time}   stones {}   damage {:.0}{}",
                            a.boxes,
                            damage,
                            if a.collateral.people > 0 {
                                format!("   people hurt {}", a.collateral.people)
                            } else {
                                String::new()
                            }
                        ))
                        .size(13.0)
                        .color(Color32::from_rgb(210, 210, 210)),
                    );
                });
        });
}

/// Won or failed: the score, and again or back to the menu
/// (Some(true): again).
fn result(
    ctx: &egui::Context,
    c: &Challenge,
    a: &Attempt,
    best: Option<(u32, u8)>,
) -> Option<bool> {
    let mut pick = None;
    egui::Window::new("challenge_result")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .min_width(360.0)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(&c.title).size(20.0).strong());
                match (&a.outcome, a.score(c)) {
                    (Outcome::Won, Some(s)) => {
                        ui.label(
                            RichText::new("★".repeat(s.stars as usize))
                                .size(36.0)
                                .color(Color32::from_rgb(250, 205, 90)),
                        );
                        ui.label(RichText::new(format!("{} points", s.points)).size(18.0));
                        ui.label(format!(
                            "time {:.0} s   stones {}   damage {:.0}",
                            a.elapsed_s,
                            a.boxes,
                            a.collateral.points()
                        ));
                        if let Some((p, _)) = best {
                            ui.label(RichText::new(format!("best {p}")).weak());
                        }
                        if c.reward.coin > 0 {
                            ui.label(format!("reward: {} coin", c.reward.coin));
                        }
                    }
                    (Outcome::Failed(why), _) => {
                        ui.label(
                            RichText::new(format!("Failed: {why}"))
                                .size(17.0)
                                .color(Color32::from_rgb(240, 120, 100)),
                        );
                    }
                    _ => {}
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Again").size(15.0)).clicked() {
                        pick = Some(true);
                    }
                    if ui.button(RichText::new("Menu").size(15.0)).clicked() {
                        pick = Some(false);
                    }
                });
            });
        });
    pick
}
