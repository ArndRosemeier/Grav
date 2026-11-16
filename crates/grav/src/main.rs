//! Grav sandbox: one spire, the hero, stones and a world that reacts.
//!
//! Keys: WASD walk, Shift sprint, Q/E turn (or look around with the mouse,
//! right button held), Space jump (push off while drifting), 1-9 take a box
//! in hand (again: put it away), left / right click more / less range, the
//! buttons under the crosshair throw or cancel, Tab the box bench, R hold
//! for stone sense.

mod boxes;
mod cavern;
mod dwarf;
mod flora;
mod goods;
mod grass;
mod heart;
mod hero;
mod link;
mod look;
mod meter;
mod pieces;
mod response;
mod rock;
mod settle;
mod shots;
mod space;
mod town;
mod visuals;
mod water;
mod wet;

use boxes::Boxes;
use engine::prelude::*;
use grav_core::{BodyQuery, BoxState, Charge, Face, Program, Tuning};
use hero::{Hero, HeroInput};
use response::Response;
use std::collections::HashMap;
use visuals::Visuals;

/// Holding the left button this long keeps adding range.
const RANGE_REPEAT_AFTER: f32 = 0.35;
/// A right press shorter than this and moving the mouse less than this
/// many pixels is a click (otherwise it turned the view).
const RIGHT_CLICK_SECONDS: f32 = 0.35;
const RIGHT_CLICK_PIXELS: f32 = 6.0;

struct Game {
    tuning: Tuning,
    base: grav_core::Field,
    grass: grass::Grass,
    scenery: space::Scenery,
    meter: meter::Meter,
    town: town::Town,
    goods: goods::Goods,
    hero: Hero,
    boxes: Boxes,
    /// Seconds the left button has been held while changing the range.
    range_held: f32,
    /// A right press in progress: seconds held and pixels the mouse moved.
    right_press: Option<(f32, f32)>,
    /// A throw button pressed in last frame's HUD.
    pressed_button: Option<boxes::ThrowButton>,
    response: Response,
    visuals: Visuals,
    waters: water::Waters,
    tour: Option<shots::Tour>,
    demo: Option<Demo>,
}

/// A scripted run for verify (no keys there): throw a box, let its clocks
/// open it over the spring and drop it, drag the hero with another.
#[derive(Default)]
struct Demo {
    step: u32,
    throw_rest: Vec3,
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
    let mut scenery = space::build_rock(world, &look)?;
    let town = town::Town::new(std::mem::take(&mut scenery.town), look.built, look.unseen);
    let goods = goods::Goods::new(std::mem::take(&mut scenery.goods), look.built);
    let base = space::base_field();
    let grass = grass::Grass::new(world, look.flora, &scenery.blades, &base)?;
    // Nothing is built here yet (the stand-in town was torn down).
    let built = space::Built::default();
    let (at, _) = space::spire_frame(0.0, -75.0);
    let mut hero = Hero::spawn(world, at + Vec3::Y * 0.1, Vec3::Y, &tuning.movement)?;
    if let Some((body, height)) = dwarf::spawn(world) {
        hero.set_animated_body(world, body, height);
    }
    let boxes = Boxes::new(world, &tuning)?;
    let response = Response::new(world, built);
    let visuals = Visuals::new(world, at)?;
    let waters = water::build(world)?;
    link::install(world, &base, HashMap::new());
    let tour = shots::Tour::from_env();
    // The scripted run opens stones; the screenshot tour wants the world alone.
    let meter = meter::Meter::new();
    let demo = (std::env::var("ENGINE_VERIFY").is_ok() && tour.is_none() && !meter.measuring())
        .then(Demo::default);
    Ok(Game {
        tuning,
        base,
        grass,
        scenery,
        meter,
        town,
        goods,
        hero,
        boxes,
        range_held: 0.0,
        right_press: None,
        pressed_button: None,
        response,
        visuals,
        waters,
        tour,
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
    // --- Boxes ---
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
        if input.pressed(key) {
            g.boxes.pick(k);
        }
    }
    if input.pressed(Key::Tab) {
        g.boxes.bench = !g.boxes.bench;
    }
    let up_now = g.hero.state(world).up;
    let up_now = if up_now.length_squared() > 0.5 {
        up_now
    } else {
        g.hero.cam_up
    };
    let hand_now = g.hero.hand(world);
    // Aim at what is under the mouse (or far along its ray). With a box in
    // hand, left click adds range and right click takes it away (holding
    // keeps changing it); the buttons under the crosshair throw or cancel.
    let (eye, mut ray) = world.cursor_ray(frame);
    if g.demo.is_some() {
        // No mouse in a verify run: aim ahead of the hero, a little up.
        ray = (feet + g.hero.heading * 12.0 + up_now * 2.5 - eye).normalize_or(ray);
    }
    let aim_point = world
        .cast_ray_ignoring(eye, ray, 120.0, hero_id)
        .map_or(eye + ray * 120.0, |h| h.point);
    let aim_dir = (aim_point - hand_now).normalize_or(g.hero.heading);
    let over_ui = frame.ui.ctx().is_pointer_over_area() || g.boxes.bench;
    let ready_to_throw = g.boxes.out && !g.boxes.bench;
    let (step, rate) = (g.tuning.flight.range_step, g.tuning.flight.range_rate);
    if ready_to_throw && !over_ui {
        if input.mouse_clicked(MouseButton::Left) {
            g.boxes.change_range(step);
            g.range_held = 0.0;
        }
        if input.mouse_down(MouseButton::Left) {
            g.range_held += dt;
            if g.range_held > RANGE_REPEAT_AFTER {
                g.boxes.change_range(rate * dt);
            }
        }
    }
    // The right button also turns the view: a short press that hardly
    // moves the mouse counts as a click.
    if input.mouse_clicked(MouseButton::Right) {
        g.right_press = Some((0.0, 0.0));
    }
    if let Some((held, moved)) = g.right_press.as_mut() {
        *held += dt;
        *moved += input.mouse_delta().length();
        if !input.mouse_down(MouseButton::Right) {
            if *held < RIGHT_CLICK_SECONDS && *moved < RIGHT_CLICK_PIXELS && ready_to_throw {
                g.boxes.change_range(-step);
            }
            g.right_press = None;
        }
    }
    let speed = g.tuning.flight.speed_for(g.boxes.range);
    let aim = ready_to_throw.then(|| {
        g.boxes.aim(
            world,
            hero_id,
            hand_now,
            aim_dir,
            speed,
            (up_now, g.hero.heading),
        )
    });
    match g.pressed_button.take() {
        Some(boxes::ThrowButton::Throw) => {
            if let Some(a) = &aim {
                g.boxes.throw(a);
            }
        }
        Some(boxes::ThrowButton::Cancel) => g.boxes.out = false,
        None => {}
    }
    if let Some(demo) = g.demo.as_mut() {
        demo.stumbled |= g.hero.stumble > 0.0;
        run_demo(demo, g.hero.id, &mut g.boxes, &g.waters, world, frame)?;
        keys = HeroInput::default();
    }
    g.boxes.tick(world, hero_id, feet, dt)?;

    // --- The field this frame ---
    let up = g.hero.state(world).up;
    let up = if up.length_squared() > 0.5 {
        up
    } else {
        g.hero.cam_up
    };
    let hand = g.hero.hand(world);
    let mut field = g.base.clone();
    field.sources = g.boxes.sources();
    if let Some((at, g_max, radius)) = g.tour.as_ref().and_then(|t| t.stone()) {
        // The screenshot tour's own open stone, for views that want one.
        field.sources.push(grav_core::Source::new(
            9000,
            Charge::Attracting,
            at,
            g_max,
            radius,
        ));
    }
    link::install(world, &field, HashMap::new());
    // The grass shows the pull: it turns to the field around open stones.
    let eye = world.camera().eye();
    g.grass.update(world, &field, eye)?;
    world.report("grass clumps", g.grass.count());

    g.hero.update(world, keys, &g.tuning.movement, dt)?;
    // Buildings a stone's field reaches become their bonded pieces.
    g.town
        .update(world, &mut g.response, &field, &g.base, &g.tuning, dt)?;
    g.goods.update(world, &field)?;
    g.response.step(world, &field, feet, dt)?;
    g.waters.update(world, feet, &field, dt)?;

    // --- Field visuals ---
    g.visuals.dust(world, &field, head, dt)?;
    g.visuals
        .sense(world, input.down(Key::R).then_some(&field), head)?;
    // The throw preview: where the box would stop, the sides it will
    // open, your fall and what would break.
    match &aim {
        Some(a) => {
            let with = g.boxes.preview_field(&field, a);
            let center = feet + up * 0.7;
            let fall = with.acceleration(&BodyQuery {
                id: hero_id.raw(),
                position: center,
                mass: 70.0,
                lodestone: None,
            });
            g.visuals.preview(world, Some((center, fall)))?;
            g.response.preview(world, Some(&with))?;
        }
        None => {
            g.visuals.preview(world, None)?;
            g.response.preview(world, None)?;
        }
    }
    let hand_frame = {
        let front = (g.hero.heading - up * g.hero.heading.dot(up))
            .normalize_or(up.any_orthonormal_vector());
        Quat::from_mat3(&engine::glam::Mat3::from_cols(up.cross(front), up, front))
    };
    g.boxes.draw(world, hand, hand_frame, aim.as_ref(), dt)?;
    if g.demo.is_some() && (3.4..3.9).contains(&frame.time) {
        // A close look at the hanging box with its front lid open.
        if let Some(at) = g.boxes.position(0) {
            let f = g.boxes.stones[0].frame;
            let eye = at + f * Vec3::new(0.7, 0.35, 0.9);
            world.look_at_up(eye, at, f * Vec3::Y);
        }
    }

    // --- HUD ---
    match g.tour.as_mut() {
        Some(tour) => tour.frame(world),
        None if g.meter.measuring() => {}
        None => hud(g, world, frame),
    }
    g.meter.frame(world, frame, &mut g.grass, &g.scenery)?;

    // --- Self-checks ---
    world.check(
        "hero is outside the spire's rock",
        !rock::shared().in_spire(feet + g.hero.cam_up * 0.6)
            && !rock::shared().in_island(feet + g.hero.cam_up * 0.6),
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
    world.report("pieces_broken", g.response.broken_total);
    world.report("buildings_woken", g.town.woken);
    world.report("buildings_asleep_again", g.town.slept);
    world.report("buildings", g.town.count().0);
    world.report("goods", g.goods.count());
    world.report("goods_loose", g.goods.loose_total);
    world.report("building_pieces", g.town.count().1);
    world.report("water_m3", g.waters.volume(world));
    Ok(())
}

fn hud(g: &mut Game, world: &mut World, frame: &Frame) {
    let ui = &frame.ui;
    ui.bar(
        Anchor::TopRight,
        "Health",
        g.hero.health,
        100.0,
        rgb(200, 60, 60),
    );
    let state = if g.hero.drifting {
        "drifting"
    } else if g.hero.walking_allowed {
        "standing"
    } else {
        "falling"
    };
    ui.text(
        Anchor::TopRight,
        format!("Pull {:.2} g, {state}", g.hero.pull_g),
    );
    if let Some((m, _)) = &g.boxes.message {
        ui.text_styled(Anchor::Center, m, 18.0, rgb(255, 220, 160));
    }
    let feet = g.hero.feet(world);
    let ctx = frame.ui.ctx();
    g.boxes.belt(ctx, feet);
    g.boxes
        .countdowns(ctx, world, frame.width as f32, frame.height as f32);
    g.boxes.bench_window(ctx);
    g.pressed_button = g.boxes.throw_buttons(ctx);
    ui.text_styled(
        Anchor::BottomLeft,
        "WASD walk  Shift sprint  Q/E turn  RMB drag look  1-9 box  LMB/RMB range  Tab bench  R sense",
        13.0,
        rgb(200, 200, 200),
    );
}

fn run_demo(
    demo: &mut Demo,
    hero: EntityId,
    boxes: &mut Boxes,
    waters: &water::Waters,
    world: &mut World,
    frame: &Frame,
) -> EngineResult<()> {
    let t = frame.time;
    let up_at = |world: &World| world.character(hero).map_or(Vec3::Y, |s| s.up);
    match demo.step {
        0 if t > 0.6 => {
            // Throw box 1 with a short wind-up: shut, it flies straight and
            // air stops it where the range says.
            boxes.pick(0);
            let up = up_at(world);
            let feet = world.world_transform(hero)?.w_axis.truncate();
            let from = feet + up * 1.0;
            let heading = (Vec3::Z - up * up.z).normalize_or(up.any_orthonormal_vector());
            let dir = (heading + up * 0.25).normalize();
            let speed = boxes.flight().speed_for(7.0);
            boxes.stones[0].program = Program::only(&[Face::Front], 0.3, 1.5);
            let aim = boxes.aim(world, hero, from, dir, speed, (up, heading));
            demo.throw_rest = aim.rest;
            boxes.throw(&aim);
            world.check(
                "a thrown box is shut in flight",
                boxes.stones[0].source(None).is_none(),
            );
            demo.step = 1;
        }
        1 if t > 3.3 => {
            let at = boxes.position(0).unwrap_or(Vec3::ZERO);
            world.report("demo_throw_miss_m", at.distance(demo.throw_rest));
            world.check(
                "a thrown box hangs where the preview said",
                matches!(
                    boxes.stones[0].state,
                    BoxState::Resting { .. } | BoxState::Dropped { .. }
                ) && at.distance(demo.throw_rest) < 0.3,
            );
            world.check("its front face has opened", boxes.stones[0].open[0]);
            demo.step = 2;
        }
        2 if t > 4.7 => {
            world.check(
                "the box came down when its drop timer ran out",
                matches!(boxes.stones[0].state, BoxState::Dropped { .. }),
            );
            demo.step = 3;
        }
        3 if t > 5.5 => {
            // Walk to it (the demo teleports) and pick it up.
            if let Some(at) = boxes.position(0) {
                let up = up_at(world);
                world.set_place(hero, place_at(at + up * 0.3))?;
            }
            demo.step = 4;
        }
        4 if t > 6.0 => {
            world.check(
                "the hero picked the dropped box up",
                boxes.stones[0].is_ready(),
            );
            // Hang box 2 over the spring with only its down face open: the
            // water rises to it.
            let pool = &waters.pool;
            let above = pool.at + pool.rot * Vec3::new(0.0, 3.5, 0.0);
            demo.pool_start = world
                .fluid(pool.fluid)
                .and_then(|f| f.center())
                .unwrap_or(pool.at);
            let s = &mut boxes.stones[1];
            s.program = Program::only(&[Face::Down], 0.0, 30.0);
            let up = pool.rot * Vec3::Y;
            let front = (pool.rot * Vec3::Z).normalize();
            let frame_rot =
                Quat::from_mat3(&engine::glam::Mat3::from_cols(up.cross(front), up, front));
            s.throw(above, Vec3::ZERO, frame_rot);
            s.rest_at(above);
            let (near, _) = space::spire_frame(16.0, -88.0);
            world.set_place(hero, place_at(near + Vec3::Y * 0.05))?;
            demo.step = 5;
        }
        5 if t > 8.5 => {
            let pool = &waters.pool;
            let above = pool.at + pool.rot * Vec3::new(0.0, 3.5, 0.0);
            let now = world
                .fluid(pool.fluid)
                .and_then(|f| f.center())
                .unwrap_or(pool.at);
            let rose = demo.pool_start.distance(above) - now.distance(above);
            world.report("demo_pool_rose_m", rose);
            world.check(
                "a box open downwards over the spring lifts its water",
                rose > 0.3,
            );
            let (quiet, _) = space::spire_frame(-20.0, -60.0);
            world.set_place(hero, place_at(quiet + Vec3::Y * 0.05))?;
            demo.step = 6;
        }
        6 if t > 9.5 => {
            // P9, P12: box 1 again, open all round and hanging 7 m to
            // the side, drags the hero to it.
            let (at, rot) = space::spire_frame(-20.0, -60.0);
            demo.drag_start = world.world_transform(hero)?.w_axis.truncate();
            demo.drag_stone = at + rot * Vec3::new(0.0, 1.0, 7.0);
            let s = &mut boxes.stones[0];
            s.program = Program::default();
            s.program.faces = [Some(0.0); 6];
            s.throw(demo.drag_stone, Vec3::ZERO, rot);
            s.rest_at(demo.drag_stone);
            demo.stumbled = false;
            demo.step = 7;
        }
        7 if t > 11.0 => {
            let now = world.world_transform(hero)?.w_axis.truncate();
            let closer = demo.drag_start.distance(demo.drag_stone) - now.distance(demo.drag_stone);
            world.report("demo_dragged_m", closer);
            world.check("an open box beside the hero drags him to it", closer > 1.5);
            world.check("a sudden pull makes the hero stumble", demo.stumbled);
            // Take the shard in hand: the preview shows.
            boxes.pick(2);
            demo.step = 8;
        }
        _ => {}
    }
    Ok(())
}

fn place_at(p: Vec3) -> Place {
    Place::new(p.x, p.y, p.z)
}
