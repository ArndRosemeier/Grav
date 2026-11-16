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
//! shows the menu in a verify run (for screenshots).

use crate::boxes::Boxes;
use crate::goods::Goods;
use crate::hero::Hero;
use crate::people::People;
use crate::response::Response;
use crate::settle::{self, Make};
use crate::town::Town;
use engine::egui::{self, Align2, Color32, RichText};
use engine::prelude::*;
use grav_core::challenge::{Constraints, Par, Reward};
use grav_core::{Attempt, Challenge, Collateral, Objective, Observed, Outcome};
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
    /// The sandbox was picked: take the goal's label away.
    leave: bool,
}

/// The ground under `p` along `-up`: walked down from above until the
/// rock begins (the physics may not know the rock yet on the first frame).
fn ground(world: &World, p: Vec3, up: Vec3) -> Vec3 {
    let rock = crate::rock::shared();
    let solid = |q: Vec3| rock.in_spire(q) || rock.in_island(q);
    let top = p + up * 25.0;
    if !solid(top) {
        let mut q = top;
        for _ in 0..240 {
            let next = q - up * 0.25;
            if solid(next) {
                return q + up * 0.1;
            }
            q = next;
        }
    }
    world
        .cast_ray(p + up * 4.0, -up, 12.0)
        .map_or(p, |hit| hit.point + up * 0.05)
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

        // Reach: up to the lantern room of an umbral tower.
        if let Some(t) = of(Make::Tower).nth(3) {
            let up = t.rot * Vec3::Y;
            let front = t.rot * Vec3::Z;
            let tall = settle::tower_tall(t.variant);
            let start = open_spot(t.at, up, 16.0, front);
            list.push(Challenge {
                id: "reach_tower".into(),
                title: "The lantern room".into(),
                brief: "Get up to the lantern room of the tall umbral tower. Its stair is locked; the pull is not.".into(),
                start: start.into(),
                start_yaw_deg: 0.0,
                objective: Objective::Reach {
                    target: (t.at + up * tall * 0.86).into(),
                    radius: 3.5,
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
                    time_s: 60.0,
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

        // Haul: a crate from the camp to the other camp's shacks... or
        // the nearest shack 15 m or more away.
        if let Some(c) = of(Make::Crate).nth(1) {
            let up = c.rot * Vec3::Y;
            let to = shacks
                .iter()
                .filter(|s| s.at.distance(c.at) > 15.0)
                .min_by(|a, b| a.at.distance(c.at).total_cmp(&b.at.distance(c.at)))
                .map(|s| s.at + s.rot * Vec3::Z * 4.0);
            if let Some(to) = to {
                let start = open_spot(c.at, up, 6.0, c.at - to);
                list.push(Challenge {
                    id: "haul_crate".into(),
                    title: "A crate for the next shack".into(),
                    brief: "Bring the crate to the shack down the road (the marked place). It is too heavy to carry: let a pull do it.".into(),
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
            leave: false,
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
        people: &People,
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
        let now = Self::tally(boxes, response, goods, people);
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
                people: (now.swept - start.swept) as u32,
            },
            boxes: now.thrown - start.thrown,
        };
        attempt.step(c, dt, &seen);
        let linked = match c.objective {
            Objective::Reach { .. } => true,
            Objective::Demolish { .. } => {
                l.target.is_some() && (c.constraints.spare.is_empty() || l.spare.is_some())
            }
            Objective::Haul { .. } => l.cargo.is_some(),
        };
        world.check("the challenge found what it is about", linked);
        if (1.5..1.7).contains(&attempt.elapsed_s) {
            world.check(
                "the hero can walk where the challenge starts",
                hero.walking_allowed,
            );
        }
        world.report("challenge_elapsed", attempt.elapsed_s);
        world.report("challenge_stones", attempt.boxes);
        world.report("challenge_damage", attempt.collateral.points());
        world.report("challenge_target_down", seen.target_down);
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
