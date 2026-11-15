//! Grav sandbox: one spire, the hero, stones and a world that reacts.
//!
//! Keys: WASD walk, Q/E turn (or look around with the mouse, right button held),
//! Space jump (push off while drifting), 1-9 pick a stone, F hold to open,
//! C hold to close (costs coating), G take in hand / pack, left click throws the
//! stone in hand to the target zone under the mouse, V set on the surface in reach, X recover, Z recall,
//! R hold for stone sense, P sandbox palette.

mod cavern;
mod dwarf;
mod heart;
mod hero;
mod link;
mod look;
mod response;
mod rock;
mod shots;
mod space;
mod stones;
mod visuals;
mod water;
mod wet;

use engine::egui;
use engine::prelude::*;
use grav_core::{BodyQuery, Charge, LodestoneBody, Placement, Profile, Tuning, G};
use hero::{Hero, HeroInput};
use response::Response;
use std::collections::HashMap;
use stones::{field_with, Inventory};
use visuals::Visuals;

struct Game {
    tuning: Tuning,
    base: grav_core::Field,
    hero: Hero,
    inv: Inventory,
    response: Response,
    visuals: Visuals,
    waters: water::Waters,
    tour: Option<shots::Tour>,
    palette: bool,
    demo: Option<Demo>,
}

/// A scripted run for verify (no keys there): set a stone by a house, open
/// it, watch tiles and timber come down, close it and pay for it.
#[derive(Default)]
struct Demo {
    step: u32,
    coating_before: f32,
    broken_open: usize,
    pool_start: Vec3,
    drag_start: Vec3,
    drag_stone: Vec3,
    stumbled: bool,
}

fn load_tuning() -> Tuning {
    match std::fs::read_to_string("tuning.json") {
        Ok(text) => Tuning::from_json(&text).unwrap_or_else(|e| {
            eprintln!("tuning.json: {e}; using the defaults");
            Tuning::default()
        }),
        Err(_) => Tuning::default(),
    }
}

fn setup(world: &mut World) -> EngineResult<Game> {
    let tuning = load_tuning();
    let look = look::Look::new(world, space::HEART_CENTER)?;
    look.light_and_air(world)?;
    world.set_physics_settings(PhysicsSettings {
        ground: PhysicsGround::None,
        ..PhysicsSettings::default()
    })?;
    space::build_rock(world, &look)?;
    let base = space::base_field();
    let built = space::build_town(world, &tuning, &base)?;
    // Warm windows: a few lamps in the street.
    for z in [-50.0, -30.0, -10.0, 10.0] {
        let (at, rot) = space::spire_frame(0.0, z);
        world.add_lamp(Lamp::point(
            at + rot * Vec3::new(0.0, 2.6, 0.0),
            rgb(255, 190, 120),
            3.0,
            14.0,
        ))?;
    }
    for x in [-180.0, 180.0] {
        let (at, rot) = space::wall_frame(x, 0.0);
        world.add_lamp(Lamp::point(
            at + rot * Vec3::new(0.0, 3.0, 3.0),
            rgb(255, 160, 90),
            3.0,
            16.0,
        ))?;
    }
    let (at, _) = space::spire_frame(0.0, -75.0);
    let mut hero = Hero::spawn(world, at + Vec3::Y * 0.1, Vec3::Y, &tuning.movement)?;
    if let Some(body) = dwarf::spawn(world) {
        hero.set_animated_body(world, body);
    }
    let inv = Inventory::new(world, &tuning)?;
    let response = Response::new(world, built);
    let visuals = Visuals::new(world, at)?;
    let waters = water::build(world)?;
    link::install(world, &base, HashMap::new());
    let tour = shots::Tour::from_env();
    // The scripted run opens stones; the screenshot tour wants the world alone.
    let demo = (std::env::var("ENGINE_VERIFY").is_ok() && tour.is_none()).then(Demo::default);
    Ok(Game {
        tuning,
        base,
        hero,
        inv,
        response,
        visuals,
        waters,
        tour,
        palette: false,
        demo,
    })
}

fn main() -> EngineResult<()> {
    let settings = Settings::default()
        .window(1280, 720)
        .quality(Quality::High)
        .vsync(false);
    let mut game: Option<Game> = None;
    Engine::run_with_settings("Grav", settings, move |world, frame| {
        if frame.first {
            game = Some(setup(world)?);
        }
        let Some(g) = game.as_mut() else {
            return Ok(());
        };
        frame_update(g, world, frame)
    })
}

fn frame_update(g: &mut Game, world: &mut World, frame: &Frame) -> EngineResult<()> {
    let dt = frame.dt;
    let input = &frame.input;
    let hero_id = g.hero.id;
    let feet = g.hero.feet(world);
    let head = g.hero.head(world);

    // --- Stone handling ---
    let mut keys = HeroInput {
        walk: input.move_xz(),
        turn: input.axis(Key::Q, Key::E) * 100.0 * dt,
        jump: input.pressed(Key::Space),
        pitch: 0.0,
        sprint: input.down(Key::Shift),
    };
    if input.mouse_down(MouseButton::Right) {
        let (yaw, pitch) = input.look_delta(0.15);
        keys.turn -= yaw;
        keys.pitch = pitch;
    }
    for (k, key) in [
        Key::Digit1,
        Key::Digit2,
        Key::Digit3,
        Key::Digit4,
        Key::Digit5,
        Key::Digit6,
        Key::Digit7,
        Key::Digit8,
        Key::Digit9,
    ]
    .into_iter()
    .enumerate()
    {
        if input.pressed(key) && k < g.inv.stones.len() {
            g.inv.selected = k;
        }
    }
    let step = Inventory::sheath_step(dt);
    if input.down(Key::F) {
        g.inv.open(hero_id, step);
    }
    if input.down(Key::C) {
        g.inv.close(step, feet, world);
    }
    if input.pressed(Key::G) {
        g.inv.hold_or_stow(hero_id);
    }
    // The target: the surface under the mouse, within throwing range of
    // the hand. It is shown whenever a stone is out, and a left click
    // throws the stone there.
    let hand_now = g.hero.hand(world);
    let holding = matches!(g.inv.current().placement, Placement::Held { .. });
    let (eye, mut ray) = world.cursor_ray(frame);
    if g.demo.is_some() {
        // No mouse in a verify run: point at the ground ahead of the hero.
        ray = (feet + g.hero.heading * 8.0 - eye).normalize_or(ray);
    }
    let under_mouse = world.cast_ray_ignoring(eye, ray, g.tuning.throw_range + 12.0, hero_id);
    let target = under_mouse.filter(|hit| hit.point.distance(hand_now) <= g.tuning.throw_range);
    if input.pressed(Key::V) {
        g.inv.set(target);
    }
    if input.pressed(Key::X) {
        g.inv.recover(hero_id, feet, world);
    }
    if input.pressed(Key::Z) {
        g.inv.recall();
    }
    let aiming = holding;
    if aiming && input.mouse_clicked(MouseButton::Left) && !frame.ui.ctx().wants_pointer_input() {
        if target.is_none() && under_mouse.is_some() {
            g.inv.say("Out of throwing range");
        } else {
            g.inv.throw(hand_now, target);
        }
    }
    if input.pressed(Key::P) {
        g.palette = !g.palette;
    }
    if let Some(demo) = g.demo.as_mut() {
        demo.stumbled |= g.hero.stumble > 0.0;
        run_demo(
            demo,
            g.hero.id,
            &mut g.inv,
            &g.response,
            &g.waters,
            world,
            frame,
        )?;
        keys = HeroInput::default();
    }
    g.inv.tick(world, dt);

    // --- The field this frame ---
    let up = g.hero.state(world).up;
    let up = if up.length_squared() > 0.5 {
        up
    } else {
        g.hero.cam_up
    };
    let hand = g.hero.hand(world);
    let field = field_with(&g.base, g.inv.sources(hero_id, hand, up));
    link::install(world, &field, HashMap::new());

    // A15: a held stone is pushed by opposite charges, and drags the hero.
    let held = g
        .inv
        .stones
        .iter()
        .find(|s| matches!(s.placement, Placement::Held { .. }) && s.opening > 0.0);
    if let Some(s) = held {
        let q = BodyQuery {
            id: hero_id.raw(),
            position: hand,
            mass: 1.0,
            lodestone: Some(LodestoneBody {
                charge: s.spec.charge,
                opening: s.opening,
                source: Some(s.id),
            }),
        };
        let ordinary = field.parts(&q).total();
        let push = field.acceleration(&q) - ordinary * (1.0 - s.opening);
        if push.length_squared() > 1e-6 {
            world.push_character(hero_id, push * dt)?;
        }
    }

    g.hero.update(world, keys, &g.tuning.movement, dt)?;
    g.response.step(world, &field, feet, dt)?;
    g.waters.update(world, feet)?;

    // --- Field visuals ---
    g.visuals.dust(world, &field, head, dt)?;
    g.visuals
        .sense(world, input.down(Key::R).then_some(&field), head)?;
    let opening_keys = input.down(Key::F) || input.down(Key::C);
    let preview = if aiming {
        target.map(|hit| {
            let s = g.inv.current();
            let mut probe = s.clone();
            probe.opening = 1.0;
            let mut with = field.clone();
            with.sources.retain(|x| x.id != s.id);
            with.sources
                .push(probe.source_at(hit.point + hit.normal * 0.15, hit.normal, None));
            (hit.point, hit.normal, with)
        })
    } else {
        None
    };
    match &preview {
        Some((landing, normal, with)) => {
            let center = feet + up * 0.7;
            let fall = with.acceleration(&BodyQuery {
                id: hero_id.raw(),
                position: center,
                mass: 70.0,
                lodestone: None,
            });
            g.visuals.preview(
                world,
                Some((
                    *landing,
                    *normal,
                    center,
                    fall,
                    g.inv.current().spec.charge == Charge::Repelling,
                )),
            )?;
            g.response.preview(world, Some(with))?;
        }
        None => {
            g.visuals.preview(world, None)?;
            g.response.preview(world, None)?;
        }
    }
    g.inv.draw(world, hand, aiming || opening_keys)?;

    // --- HUD ---
    match g.tour.as_mut() {
        Some(tour) => tour.frame(world),
        None => hud(g, world, frame),
    }

    // --- Self-checks ---
    world.check(
        "hero is outside the spire's rock",
        !rock::shared().in_spire(feet + g.hero.cam_up * 0.6),
    );
    world.check(
        "field is finite at the hero",
        field.acceleration(&BodyQuery::at(head)).is_finite(),
    );
    if (0.3..0.5).contains(&frame.time) {
        // Before the demo moves anything: the hero has landed and may walk.
        world.check(
            "hero stands and can walk at the start",
            g.hero.walking_allowed,
        );
    }
    world.report("hero_pull_g", g.hero.pull_g);
    world.report("coating", g.inv.coating.units);
    world.report("pieces_broken", g.response.broken_total);
    world.report("water_m3", g.waters.volume(world));
    Ok(())
}

fn hud(g: &mut Game, world: &mut World, frame: &Frame) {
    let ui = &frame.ui;
    let s = g.inv.current();
    let charge = match s.spec.charge {
        Charge::Attracting => "attracting",
        Charge::Repelling => "repelling",
    };
    let place = match s.placement {
        Placement::Packed => "packed",
        Placement::Held { .. } => "in hand",
        Placement::Set { .. } => "set",
        Placement::Loose { .. } => "flying",
    };
    ui.text(
        Anchor::TopLeft,
        format!(
            "Stone {} of {}: {} ({charge}, {:.1} g, {:.0} m, {:?})",
            g.inv.selected + 1,
            g.inv.stones.len(),
            s.spec.name,
            s.spec.max_g,
            s.spec.radius,
            s.spec.profile
        ),
    );
    ui.bar(
        Anchor::TopLeft,
        "Sheath open",
        s.opening,
        1.0,
        rgb(230, 170, 70),
    );
    ui.text(Anchor::TopLeft, format!("It is {place}"));
    if g.inv.coating.unlimited {
        ui.text(Anchor::TopLeft, "Coating: unlimited");
    } else {
        ui.bar(
            Anchor::TopLeft,
            "Coating",
            g.inv.coating.units,
            g.tuning.start_coating.max(g.inv.coating.units),
            rgb(170, 170, 200),
        );
    }
    ui.bar(
        Anchor::TopRight,
        "Health",
        g.hero.health,
        100.0,
        rgb(200, 60, 60),
    );
    let state = if g.hero.drifting {
        "drifting".to_string()
    } else if g.hero.walking_allowed {
        "standing".to_string()
    } else {
        "falling".to_string()
    };
    ui.text(
        Anchor::TopRight,
        format!("Pull {:.2} g, {state}", g.hero.pull_g),
    );
    ui.text(
        Anchor::TopRight,
        format!(
            "Broken pieces {}, warning {}",
            g.response.broken_total, g.response.warning_now
        ),
    );
    if let Some((m, _)) = &g.inv.message {
        ui.text_styled(Anchor::Center, m, 18.0, rgb(255, 220, 160));
    }
    ui.text_styled(
        Anchor::Bottom,
        "WASD walk  Shift sprint  Q/E turn  Space jump  1-9 stone  F open  C close  G hold/pack  Click throw  V set  X recover  Z recall  R sense  P palette",
        14.0,
        rgb(220, 220, 220),
    );
    if g.palette {
        palette(g, world, frame);
    }
}

/// Sandbox palette: dress the selected stone freely, coating on or off.
fn palette(g: &mut Game, _world: &mut World, frame: &Frame) {
    let t = &g.tuning;
    let (max_g, max_r) = (t.loot_max_g, t.loot_max_radius);
    let mut unlimited = g.inv.coating.unlimited;
    let (selected, count, pull) = (g.inv.selected, g.inv.stones.len(), g.hero.pull_g);
    let s = &mut g.inv.stones[selected];
    egui::Window::new("Sandbox palette").show(frame.ui.ctx(), |ui| {
        ui.label(format!("Stone {}", selected + 1));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.spec.charge, Charge::Attracting, "Attracting");
            ui.selectable_value(&mut s.spec.charge, Charge::Repelling, "Repelling");
        });
        ui.horizontal_wrapped(|ui| {
            for p in [
                Profile::Core,
                Profile::Leash,
                Profile::Shell,
                Profile::Uniform,
                Profile::Plane,
                Profile::Line,
            ] {
                ui.selectable_value(&mut s.spec.profile, p, format!("{p:?}"));
            }
        });
        ui.add(egui::Slider::new(&mut s.spec.max_g, 0.1..=max_g).text("max g"));
        ui.add(egui::Slider::new(&mut s.spec.radius, 5.0..=max_r).text("radius m"));
        let mut pulse = matches!(s.spec.time, grav_core::TimeDial::Pulse { .. });
        let mut rhythm = matches!(s.spec.time, grav_core::TimeDial::Rhythm { .. });
        ui.horizontal(|ui| {
            if ui.checkbox(&mut pulse, "Pulse 1 s on / 1 s off").changed() {
                s.spec.time = if pulse {
                    grav_core::TimeDial::Pulse { on: 1.0, off: 1.0 }
                } else {
                    grav_core::TimeDial::Constant
                };
            }
            if ui.checkbox(&mut rhythm, "Rhythm 4 s").changed() {
                s.spec.time = if rhythm {
                    grav_core::TimeDial::Rhythm { period: 4.0 }
                } else {
                    grav_core::TimeDial::Constant
                };
            }
        });
        let mut heavy_only = matches!(s.spec.selectivity, grav_core::Selectivity::MassBand { .. });
        if ui
            .checkbox(&mut heavy_only, "Only bodies over 100 kg")
            .changed()
        {
            s.spec.selectivity = if heavy_only {
                grav_core::Selectivity::MassBand {
                    min: 100.0,
                    max: f32::INFINITY,
                }
            } else {
                grav_core::Selectivity::All
            };
        }
        if ui
            .checkbox(&mut s.spec.spring_sheath, "Spring sheath (recall)")
            .changed()
        {
            s.refill_spring();
        }
        ui.checkbox(&mut unlimited, "Unlimited coating");
        ui.label(format!("{count} stones; pull at the hero {pull:.2} g"));
    });
    if unlimited != g.inv.coating.unlimited {
        g.inv.coating = if unlimited {
            grav_core::Coating::unlimited()
        } else {
            grav_core::Coating::new(g.tuning.start_coating)
        };
    }
}

fn run_demo(
    demo: &mut Demo,
    hero: EntityId,
    inv: &mut Inventory,
    response: &Response,
    waters: &water::Waters,
    world: &mut World,
    frame: &Frame,
) -> EngineResult<()> {
    let t = frame.time;
    match demo.step {
        0 if t > 0.5 => {
            // Set the first stone right next to the house at z = 0, and open it.
            inv.selected = 0;
            inv.hold_or_stow(hero);
            let (at, rot) = space::spire_frame(-9.0, 0.0);
            let target = at + rot * Vec3::new(0.0, 2.5, -3.5);
            inv.stones[0].placement = Placement::Set {
                at: target,
                axis: rot * Vec3::Y,
                on: None,
            };
            inv.stones[0].open_to(1.0);
            demo.step = 1;
        }
        1 if t > 2.5 => {
            demo.broken_open = response.broken_total;
            world.check(
                "an open stone strips tiles and timber nearby",
                response.broken_total > 0,
            );
            // Walk the hero over (the demo teleports): closing needs reach.
            let (at, rot) = space::spire_frame(-9.0, 0.0);
            let near = at + rot * Vec3::new(0.0, 0.0, -4.2);
            world.set_place(hero, Place::new(near.x, near.y + 0.05, near.z))?;
            demo.step = 2;
        }
        2 if t > 3.0 => {
            demo.coating_before = inv.coating.units;
            inv.selected = 0;
            let feet = world.world_transform(hero)?.w_axis.truncate();
            inv.close(1.0, feet, world);
            let paid = demo.coating_before - inv.coating.units;
            world.check(
                "closing a fully open 1.5 g stone costs 1.5 coating (S9)",
                (paid - 1.5).abs() < 1e-3,
            );
            world.report("demo_paid", paid);
            demo.step = 3;
        }
        3 if t > 4.0 => {
            world.report("demo_broken_while_open", demo.broken_open);
            let s = world.character(hero).unwrap_or_default();
            world.check(
                "hero stands on the spire after the demo",
                s.grounded || s.gravity.length() > 0.5 * G,
            );
            demo.step = 4;
        }
        4 if t > 4.5 => {
            // Hang the second stone over the pool and open it: the water
            // rises to it.
            let pool = &waters.pool;
            let above = pool.at + pool.rot * Vec3::new(0.0, 3.5, 0.0);
            demo.pool_start = world
                .fluid(pool.fluid)
                .and_then(|f| f.center())
                .unwrap_or(pool.at);
            inv.stones[1].placement = Placement::Set {
                at: above,
                axis: pool.rot * Vec3::NEG_Y,
                on: None,
            };
            inv.stones[1].open_to(1.0);
            // Walk over to watch (the demo teleports).
            let (near, _) = space::spire_frame(16.0, -88.0);
            world.set_place(hero, Place::new(near.x, near.y + 0.05, near.z))?;
            demo.step = 5;
        }
        5 if t > 7.0 => {
            let pool = &waters.pool;
            let above = pool.at + pool.rot * Vec3::new(0.0, 3.5, 0.0);
            let now = world
                .fluid(pool.fluid)
                .and_then(|f| f.center())
                .unwrap_or(pool.at);
            let rose = demo.pool_start.distance(above) - now.distance(above);
            world.report("demo_pool_rose_m", rose);
            world.check("an open stone over the pool lifts its water", rose > 0.3);
            // Close it, and stand somewhere quiet for the next part.
            inv.stones[1].opening = 0.0;
            let (quiet, _) = space::spire_frame(-20.0, -60.0);
            world.set_place(hero, Place::new(quiet.x, quiet.y + 0.05, quiet.z))?;
            demo.step = 6;
        }
        6 if t > 8.0 => {
            // P9, P12: a stone opened 7 m to the side drags the hero to it.
            let (at, rot) = space::spire_frame(-20.0, -60.0);
            demo.drag_start = world.world_transform(hero)?.w_axis.truncate();
            demo.drag_stone = at + rot * Vec3::new(0.0, 1.0, 7.0);
            inv.stones[0].placement = Placement::Set {
                at: demo.drag_stone,
                axis: rot * Vec3::Y,
                on: None,
            };
            inv.stones[0].open_to(1.0);
            demo.stumbled = false;
            demo.step = 7;
        }
        7 if t > 9.5 => {
            let now = world.world_transform(hero)?.w_axis.truncate();
            let closer = demo.drag_start.distance(demo.drag_stone) - now.distance(demo.drag_stone);
            world.report("demo_dragged_m", closer);
            world.check(
                "an open stone beside the hero drags him to it",
                closer > 1.5,
            );
            world.check("a sudden pull makes the hero stumble", demo.stumbled);
            inv.stones[0].opening = 0.0;
            // Take the shard in hand: the target zone shows under the mouse.
            inv.selected = 2;
            inv.hold_or_stow(hero);
            demo.step = 8;
        }
        _ => {}
    }
    Ok(())
}
