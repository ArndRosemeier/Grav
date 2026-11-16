//! Where people live, and what they build (design doc, "The peoples build
//! for their pull").
//!
//! - Umbrals, on the spire (1 g and less): thin tall towers of pale foamed
//!   rock in a lattice of metal, far too slender to stand in dwarf
//!   country, joined high up by light bridges. Their town lines the road
//!   along the spire's settled top.
//! - Dwarves, on the outer wall (1.8 g): squat holds of heavy stone
//!   blocks, walls leaning in, buttressed, under low iron-bound vaults.
//!   Their street is the lane on the wall.
//! - Goblins, on the seam between (the flare, by the ring road): shacks
//!   on crooked poles, patched together from plates, planks and cloth.
//!
//! A settlement sits on settled ground, by a road, clear of the water.
//! Each building is made once per shape, at three levels of detail and as
//! a plain solid for collision, and drawn wherever one stands.

use crate::flora::{arc, fine, hash, smooth, solid, Form, Stand, Stuff};
use crate::pieces::{shatter, Cells, Hold, Shard};
use crate::rock::{self, Rock, Waters};
use crate::space;
use engine::prelude::*;
use grav_core::BondClass;
use std::f32::consts::{PI, TAU};

/// What can stand in a settlement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Make {
    Tower,
    Bridge,
    Hold,
    Shack,
    Lamp,
    Brazier,
    /// Loose goods (goods.rs).
    Jar,
    Cask,
    Crate,
    Sack,
}

impl Make {
    /// Loose goods: not built, they only stand there.
    pub fn loose(self) -> bool {
        matches!(self, Make::Jar | Make::Cask | Make::Crate | Make::Sack)
    }
}

const MAKES: [Make; 10] = [
    Make::Tower,
    Make::Bridge,
    Make::Hold,
    Make::Shack,
    Make::Lamp,
    Make::Brazier,
    Make::Jar,
    Make::Cask,
    Make::Crate,
    Make::Sack,
];

/// One thing built: what, which shape of it, where and how it is turned
/// (+y up, +z its front), and how far around it nothing grows.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub make: Make,
    pub variant: u32,
    pub at: Vec3,
    pub rot: Quat,
    pub clear: f32,
}

/// How many shapes there are of each thing.
fn variants(make: Make) -> u32 {
    match make {
        Make::Tower => 4,
        Make::Bridge | Make::Hold | Make::Shack => 3,
        Make::Lamp | Make::Brazier => 1,
        Make::Jar | Make::Cask | Make::Crate | Make::Sack => 2,
    }
}

/// How tall tower shape `variant` stands.
fn tower_height(variant: u32) -> f32 {
    [13.0, 19.0, 27.0, 38.0][variant as usize % 4]
}

/// A tower's radius at its foot.
fn tower_foot(variant: u32) -> f32 {
    1.75 + 0.03 * tower_height(variant)
}

/// How long bridge shape `variant` is.
fn bridge_length(variant: u32) -> f32 {
    [8.0, 10.0, 12.0][variant as usize % 3]
}

/// The height bridges cross at.
const BRIDGE_HEIGHT: f32 = 7.6;

/// A hold's width along its front and its depth.
fn hold_size(variant: u32) -> (f32, f32) {
    let k = (variant + 1) * 613;
    (8.0 + 5.0 * hash(k, 1), 6.5 + 2.5 * hash(k, 2))
}

/// Where everything stands: the umbral town on the spire, the dwarven
/// street on the wall and the goblin camp on the seam.
pub fn plan(rock: &Rock, waters: &Waters) -> Vec<Site> {
    let mut sites = Vec::new();
    // Standing or running water nobody builds in.
    let wet: Vec<Vec3> = (0..waters.spire.at.len())
        .filter(|&i| waters.spire_water.wet[i] > 0.75)
        .map(|i| waters.spire.at[i])
        .collect();
    let dry = |p: Vec3, reach: f32| !wet.iter().any(|w| w.distance(p) < reach);

    // The umbral town: towers on both sides of the road along the top of
    // the spire, the low ones by the road and the tall ones behind them.
    for side in [-1.0_f32, 1.0] {
        let turn = Quat::from_rotation_y(-side * PI / 2.0);
        let key = if side > 0.0 { 100 } else { 200 };
        let mut z = -103.0 + 4.0 * hash(key, 1);
        let mut last: Option<(f32, f32)> = None;
        let mut n = 0;
        while z < 54.0 {
            n += 1;
            let k = key + n * 7;
            let variant = if hash(k, 2) < 0.2 {
                2
            } else {
                (hash(k, 3) * 2.0) as u32
            };
            let span = (hash(k, 4) * 3.0) as u32 % 3;
            let off = 7.5 + tower_foot(variant) + 1.5 * hash(k, 5);
            let angle = (side * off / space::SPIRE_RADIUS).to_degrees();
            let (at, rot) = space::spire_frame(angle, z);
            if dry(at, 3.6) {
                sites.push(Site {
                    make: Make::Tower,
                    variant,
                    at,
                    rot: rot * turn,
                    clear: 4.5,
                });
                // A bridge to the tower before, if that one stands the
                // right distance back and at the same turn.
                if let Some((before, before_angle)) = last {
                    for b in 0..3 {
                        let fits = (z - before - bridge_length(b) - 3.5).abs() < 0.3;
                        if fits && hash(k, 6) < 0.9 {
                            let mid = (angle + before_angle) * 0.5;
                            let (foot, rot) = space::spire_frame(mid, (z + before) * 0.5);
                            sites.push(Site {
                                make: Make::Bridge,
                                variant: b,
                                at: foot + rot * Vec3::Y * BRIDGE_HEIGHT,
                                rot,
                                clear: 0.0,
                            });
                        }
                    }
                }
                last = Some((z, angle));
            } else {
                last = None;
            }
            // The tall ones behind, where the row has room.
            if hash(k, 7) < 0.55 {
                let variant = 2 + (hash(k, 8) * 2.0) as u32 % 2;
                let off = 21.0 + 5.0 * hash(k, 9);
                let angle = (side * off / space::SPIRE_RADIUS).to_degrees();
                let (at, rot) = space::spire_frame(angle, z + 5.0 * hash(k, 10));
                if dry(at, 4.2) {
                    sites.push(Site {
                        make: Make::Tower,
                        variant,
                        at,
                        rot: rot * turn,
                        clear: 5.5,
                    });
                }
            }
            z += bridge_length(span) + 3.5;
        }
        // Lamps along the road.
        let mut z = -106.0 + 6.5 * (side + 1.0) * 0.5;
        while z < 58.0 {
            let (at, rot) = space::spire_frame((side * 4.4 / space::SPIRE_RADIUS).to_degrees(), z);
            if dry(at, 2.0) {
                sites.push(Site {
                    make: Make::Lamp,
                    variant: 0,
                    at,
                    rot: rot * turn,
                    clear: 1.0,
                });
            }
            z += 13.0;
        }
    }

    // The dwarven street: holds shoulder to shoulder on both sides of the
    // lane on the wall, fronts to the lane.
    for end in [-1.0_f32, 1.0] {
        for side in [-1.0_f32, 1.0] {
            let key = 300 + ((end + 1.0) * 2.0 + side + 1.0) as u32 * 50;
            let mut x = 172.0;
            let mut n = 0;
            while x < 226.0 {
                n += 1;
                let variant = (hash(key + n, 1) * 3.0) as u32 % 3;
                let (w, d) = hold_size(variant);
                let (at, rot) = space::wall_frame(end * (x + w * 0.5), side * (8.2 + d * 0.5));
                let turn = if side > 0.0 { 0.0 } else { PI };
                sites.push(Site {
                    make: Make::Hold,
                    variant,
                    at,
                    rot: rot * Quat::from_rotation_y(turn),
                    clear: w * 0.5 + 2.0,
                });
                x += w + 1.8 + 2.5 * hash(key + n, 2);
            }
            let mut x = 170.0;
            while x < 232.0 {
                let (at, rot) = space::wall_frame(end * x, side * 5.6);
                sites.push(Site {
                    make: Make::Brazier,
                    variant: 0,
                    at,
                    rot,
                    clear: 1.0,
                });
                x += 12.5;
            }
        }
    }

    // The goblin camps: on the flare by the ring road, where the spire's
    // pull and the wall's meet, shacks on both sides of the road.
    let field = space::base_field();
    for (c, camp) in [50.0_f32, 228.0].into_iter().enumerate() {
        for i in 0..7 {
            let k = 500 + c as u32 * 40 + i;
            let inner = i % 2 == 0;
            let rho = rock::RING_ROAD + if inner { -6.5 } else { 6.5 } + 1.5 * (hash(k, 1) - 0.5);
            let theta = camp.to_radians() + (i as f32 - 3.0) * 5.6 / rock::RING_ROAD;
            let Some(at) = rock.flare_at(theta, rho) else {
                continue;
            };
            let pull = field.acceleration(&grav_core::BodyQuery::at(at));
            let up = (-pull).normalize_or(Vec3::Z);
            let out = Vec3::new(theta.sin(), theta.cos(), 0.0) * if inner { 1.0 } else { -1.0 };
            let front = (out - up * out.dot(up)).normalize_or(up.any_orthonormal_vector());
            let rot =
                grav_core::facing(up, front) * Quat::from_rotation_y((hash(k, 2) - 0.5) * 0.7);
            sites.push(Site {
                make: Make::Shack,
                variant: (hash(k, 3) * 3.0) as u32 % 3,
                at,
                rot,
                clear: 3.2,
            });
        }
    }
    wares(rock, &mut sites);
    // Places to look at in the screenshot tour.
    let mut marks = Vec::new();
    for (make, name, nth) in [
        (Make::Tower, "built: an umbral tower", 3),
        (Make::Bridge, "built: a bridge between towers", 1),
        (Make::Hold, "built: a dwarven hold", 2),
        (Make::Shack, "built: a goblin shack", 2),
        (Make::Jar, "goods: jars by a tower", 4),
        (Make::Cask, "goods: casks by a hold", 3),
        (Make::Crate, "goods: crates by a shack", 1),
        (Make::Sack, "goods: sacks by a shack", 1),
    ] {
        if let Some(s) = sites.iter().filter(|s| s.make == make).nth(nth) {
            marks.push((name, s.at, s.rot * Vec3::Y, s.rot * Vec3::Z));
        }
    }
    let _ = LANDMARKS.set(marks);
    sites
}

/// Loose goods by the buildings' fronts: jars by the towers, casks by the
/// holds, crates and sacks by the shacks, a few together.
fn wares(rock: &Rock, sites: &mut Vec<Site>) {
    let field = space::base_field();
    let built: Vec<Site> = sites.clone();
    for (n, s) in built.iter().enumerate() {
        let k = 9000 + n as u32 * 17;
        // What, how far out from the middle its front is, how wide that is.
        let (kinds, front, wide): (&[Make], f32, f32) = match s.make {
            Make::Tower => (
                &[Make::Jar],
                tower_foot(s.variant) + 0.45,
                tower_foot(s.variant) * 0.8,
            ),
            Make::Hold => {
                let (w, d) = hold_size(s.variant);
                (&[Make::Cask], d * 0.5 + 0.55, w * 0.4)
            }
            Make::Shack => (&[Make::Crate, Make::Sack], 2.7, 1.8),
            _ => continue,
        };
        let count = 1 + (hash(k, 1) * 4.0) as u32;
        let spot = (hash(k, 2) * 2.0 - 1.0) * wide;
        for m in 0..count {
            let km = k + m * 5 + 1;
            let local = Vec3::new(
                spot + (m as f32 - count as f32 * 0.5) * 0.78 + 0.2 * (hash(km, 3) - 0.5),
                0.0,
                front + 0.45 * hash(km, 4),
            );
            let p = s.at + s.rot * local;
            let theta = p.x.atan2(p.y);
            let ground = match s.make {
                Make::Tower => Some(rock.spire_point(theta, p.z)),
                Make::Hold => Some(rock.wall_point(p.x, p.y)),
                _ => rock.flare_at(theta, p.truncate().length()),
            };
            let Some(at) = ground else { continue };
            // Not inside the next building.
            if built
                .iter()
                .any(|b| b.at.distance(at) < b.clear * 0.8 && b.at.distance(s.at) > 0.01)
            {
                continue;
            }
            let pull = field.acceleration(&grav_core::BodyQuery::at(at));
            let up = (-pull).normalize_or(s.rot * Vec3::Y);
            sites.push(Site {
                make: kinds[(hash(km, 6) * kinds.len() as f32) as usize % kinds.len()],
                variant: (hash(km, 7) * 2.0) as u32 % 2,
                at: at - up * 0.02,
                rot: Quat::from_rotation_arc(Vec3::Y, up)
                    * Quat::from_rotation_y(hash(km, 5) * TAU),
                clear: 0.45,
            });
        }
    }
}

/// A place worth a look: its name, where, its up, its front.
type Mark = (&'static str, Vec3, Vec3, Vec3);
static LANDMARKS: std::sync::OnceLock<Vec<Mark>> = std::sync::OnceLock::new();

/// Buildings to look at in the screenshot tour (known once `plan` has run).
pub fn landmarks() -> &'static [Mark] {
    LANDMARKS.get().map_or(&[], |m| m.as_slice())
}

// ---------------------------------------------------------------------
// What a thing is made of (the material reads it from uv.y).
pub(crate) const FOAM: f32 = 0.0;
pub(crate) const LATTICE: f32 = 1.0;
pub(crate) const BLOCKS: f32 = 2.0;
pub(crate) const IRON: f32 = 3.0;
pub(crate) const WOOD: f32 = 4.0;
pub(crate) const PANE: f32 = 5.0;
pub(crate) const CLOTH: f32 = 6.0;

const UP: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];

/// A window: a lit pane with a point at the top, set on a wall at
/// `center` facing `out`, in a frame.
#[allow(clippy::too_many_arguments)]
fn pointed_window(
    f: &mut Form,
    center: Vec3,
    right: Vec3,
    up: Vec3,
    out: Vec3,
    wide: f32,
    tall: f32,
    light: (Vec3, f32),
    frame: Option<(f32, Vec3)>,
) -> EngineResult<()> {
    let (w, h) = (wide * 0.5, tall * 0.5);
    let outline = [
        center - right * w - up * h,
        center + right * w - up * h,
        center + right * w + up * h * 0.35,
        center + up * h,
        center - right * w + up * h * 0.35,
    ];
    f.surface = PANE;
    f.face(&outline, center - out, light)?;
    if let Some((thick, color)) = frame {
        f.surface = LATTICE;
        let mut path: Vec<Vec3> = outline.iter().map(|&p| p + out * thick * 0.6).collect();
        path.push(path[0]);
        f.tube(&path, 5, &|_| thick, &|_| (color, 0.0))?;
    }
    Ok(())
}

/// An umbral tower: a slender body of pale foamed rock, waisted and
/// swelling again under its crown, in a lattice of metal that winds
/// around it both ways; lit windows that climb around it, a balcony, a
/// lantern room and a needle.
fn tower(variant: u32, detail: u32) -> EngineResult<Form> {
    let mut f = Form::new();
    let k = (variant + 1) * 733;
    let tall = tower_height(variant);
    let foot = tower_foot(variant);
    let radius = |y: f32| {
        let t = (y / tall).clamp(0.0, 1.0);
        let body = 1.0 + 0.32 * (1.0 - smooth(0.0, 0.09, t)) - 0.24 * smooth(0.04, 0.5, t)
            + 0.36 * smooth(0.58, 0.82, t);
        foot * body * (1.0 - smooth(0.84, 1.0, t)).powf(0.62)
    };
    let foam = Vec3::new(0.56, 0.55, 0.5).lerp(Vec3::new(0.5, 0.56, 0.58), hash(k, 1));
    let metal = Vec3::new(0.1, 0.19, 0.16);
    let light = (Vec3::new(0.55, 1.0, 0.8), 1.25);
    let steps = fine(detail, 46, 22, 10);
    let profile: Vec<(f32, f32)> = (0..=steps)
        .map(|i| {
            let y = -0.6 + (tall + 0.6) * i as f32 / steps as f32;
            (radius(y).max(0.0), y)
        })
        .collect();
    f.surface = FOAM;
    f.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &profile,
        fine(detail, 36, 18, 9),
        &|_, angle| 1.0 + 0.012 * (angle * 9.0).cos(),
        &|ring, _| {
            let t = ring as f32 / steps as f32;
            (foam * (0.82 + 0.3 * t), 0.0)
        },
    )?;
    let ring_point = |angle: f32, y: f32, off: f32| {
        Vec3::new(angle.sin(), 0.0, angle.cos()) * (radius(y) + off) + Vec3::Y * y
    };
    // The balcony.
    let deck = tall * 0.6;
    let reach = 1.15;
    let r = radius(deck);
    f.lathe(
        Vec3::ZERO,
        Vec3::Y,
        &[
            (r * 0.9, deck - 0.5),
            (r + reach * 0.75, deck - 0.14),
            (r + reach, deck - 0.05),
            (r + reach, deck + 0.05),
            (r * 0.9, deck + 0.05),
        ],
        fine(detail, 36, 18, 9),
        &|_, _| 1.0,
        &|_, _| (foam * 0.9, 0.0),
    )?;
    if solid(detail) {
        return Ok(f);
    }
    // The needle.
    f.surface = LATTICE;
    f.tube(
        &[
            Vec3::Y * (tall - 0.5),
            Vec3::Y * (tall * 1.1),
            Vec3::Y * (tall * 1.2),
        ],
        fine(detail, 8, 5, 3),
        &|t| 0.14 * (1.0 - 0.85 * t),
        &|_| (metal, 0.0),
    )?;
    f.surface = PANE;
    f.drop(
        Vec3::Y * (tall * 1.2),
        Vec3::Y,
        0.16,
        0.5,
        fine(detail, 12, 7, 4),
        light,
    )?;
    // Windows, a few to a storey, each storey turned on from the last.
    let mut y = 4.4;
    let mut storey = 0;
    while y < tall * 0.52 {
        let count = 3 + (hash(k + storey, 2) * 2.0) as u32;
        for w in 0..count {
            let angle = w as f32 / count as f32 * TAU + storey as f32 * 0.7 + 0.5;
            let out = Vec3::new(angle.sin(), 0.0, angle.cos());
            if hash(k + storey * 9 + w, 3) < 0.25 {
                continue;
            }
            pointed_window(
                &mut f,
                ring_point(angle, y, 0.03),
                Vec3::Y.cross(out),
                Vec3::Y,
                out,
                0.42,
                1.5,
                (light.0, light.1 * (0.5 + 0.7 * hash(k + storey * 9 + w, 4))),
                (detail == 0).then_some((0.035, metal)),
            )?;
        }
        y += 3.3;
        storey += 1;
    }
    // The lantern room under the crown: tall panes all round.
    let room = tall * 0.76;
    let panes = 8;
    for w in 0..panes {
        let angle = (w as f32 + 0.5) / panes as f32 * TAU;
        let out = Vec3::new(angle.sin(), 0.0, angle.cos());
        pointed_window(
            &mut f,
            ring_point(angle, room, 0.03),
            Vec3::Y.cross(out),
            Vec3::Y,
            out,
            radius(room) * 0.5,
            tall * 0.13,
            (light.0, 1.7),
            (detail < 2).then_some((0.05, metal)),
        )?;
    }
    // The door, to the front.
    let door = ring_point(0.0, 1.1, 0.02);
    f.surface = IRON;
    pointed_window(
        &mut f,
        door,
        Vec3::X,
        Vec3::Y,
        Vec3::Z,
        1.0,
        2.4,
        (Vec3::new(0.05, 0.07, 0.07), 0.0),
        (detail < 2).then_some((0.07, metal)),
    )?;
    if detail == 2 {
        return Ok(f);
    }
    // The lattice: bars winding up it both ways, standing off the rock.
    let bars = fine(detail, 7, 5, 0);
    let winds = 0.9 + 0.5 * hash(k, 5);
    f.surface = LATTICE;
    for hand in [-1.0_f32, 1.0] {
        for b in 0..bars {
            let n = fine(detail, 44, 16, 0);
            let path: Vec<Vec3> = (0..=n)
                .map(|i| {
                    let t = i as f32 / n as f32;
                    let y = 0.3 + (tall * 0.84 - 0.3) * t;
                    ring_point(
                        b as f32 / bars as f32 * TAU + hand * t * winds * TAU,
                        y,
                        0.2,
                    )
                })
                .collect();
            f.tube(
                &path,
                fine(detail, 6, 4, 3),
                &|_| if detail == 0 { 0.045 } else { 0.06 },
                &|_| (metal, 0.0),
            )?;
        }
    }
    // The balcony's rail, and lanterns hanging under its edge.
    let around = fine(detail, 36, 14, 8);
    for (height, thick) in [(1.05, 0.04), (0.55, 0.025)] {
        let rail: Vec<Vec3> = (0..=around)
            .map(|i| {
                ring_point(i as f32 / around as f32 * TAU, deck, reach - 0.06) + Vec3::Y * height
            })
            .collect();
        f.tube(&rail, fine(detail, 6, 4, 3), &|_| thick, &|_| (metal, 0.0))?;
        if detail > 0 {
            break;
        }
    }
    if detail == 0 {
        for i in 0..18 {
            let foot = ring_point(i as f32 / 18.0 * TAU, deck, reach - 0.06);
            f.tube(
                &[foot, foot + Vec3::Y * 0.5, foot + Vec3::Y * 1.05],
                5,
                &|_| 0.022,
                &|_| (metal, 0.0),
            )?;
        }
    }
    for i in 0..5 {
        let hook = ring_point((i as f32 + 0.3) / 5.0 * TAU, deck, reach - 0.1) - Vec3::Y * 0.1;
        f.surface = LATTICE;
        f.tube(
            &[hook, hook - Vec3::Y * 0.35, hook - Vec3::Y * 0.7],
            4,
            &|_| 0.012,
            &|_| (metal, 0.0),
        )?;
        f.surface = PANE;
        f.drop(
            hook - Vec3::Y * 0.7,
            -Vec3::Y,
            0.11,
            0.3,
            fine(detail, 12, 7, 4),
            light,
        )?;
    }
    Ok(f)
}

/// A bridge between two towers: a thin deck that arches up, light rails,
/// and a bowed chord under it with struts.
fn bridge(variant: u32, detail: u32) -> EngineResult<Form> {
    let mut f = Form::new();
    let length = bridge_length(variant) + 0.9;
    let n = fine(detail, 16, 8, 4);
    let foam = Vec3::new(0.54, 0.54, 0.5);
    let metal = Vec3::new(0.1, 0.19, 0.16);
    let at =
        |t: f32, x: f32, lift: f32| Vec3::new(x, 0.75 * (PI * t).sin() + lift, (t - 0.5) * length);
    f.surface = FOAM;
    for i in 0..n {
        let (a, b) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        f.block(
            [
                at(a, -0.6, -0.1),
                at(a, 0.6, -0.1),
                at(b, 0.6, -0.1),
                at(b, -0.6, -0.1),
            ],
            [
                at(a, -0.6, 0.0),
                at(a, 0.6, 0.0),
                at(b, 0.6, 0.0),
                at(b, -0.6, 0.0),
            ],
            (foam, 0.0),
        )?;
    }
    if solid(detail) {
        return Ok(f);
    }
    f.surface = LATTICE;
    for x in [-0.57_f32, 0.57] {
        let rail: Vec<Vec3> = (0..=n).map(|i| at(i as f32 / n as f32, x, 1.0)).collect();
        f.tube(&rail, fine(detail, 6, 4, 3), &|_| 0.035, &|_| (metal, 0.0))?;
        if detail == 2 {
            continue;
        }
        let chord: Vec<Vec3> = (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32;
                at(t, x, -0.15 - 0.9 * (PI * t).sin())
            })
            .collect();
        f.tube(&chord, fine(detail, 6, 4, 3), &|_| 0.04, &|_| (metal, 0.0))?;
        for (i, &under) in chord.iter().enumerate() {
            let t = i as f32 / n as f32;
            f.tube(
                &[at(t, x, 0.0), at(t, x, 0.5), at(t, x, 1.0)],
                4,
                &|_| 0.02,
                &|_| (metal, 0.0),
            )?;
            if i > 0 && i < n {
                f.tube(&[under, at(t, x, -0.05)], 4, &|_| 0.022, &|_| (metal, 0.0))?;
            }
        }
    }
    Ok(f)
}

/// A dwarven hold: a squat block of heavy stone, its walls leaning in,
/// buttressed at the corners, on a plinth, under a low vault bound in
/// iron; a round-arched porch, small deep windows, a thick stack.
fn hold(variant: u32, detail: u32) -> EngineResult<Form> {
    let mut f = Form::new();
    let k = (variant + 1) * 613;
    let (w, d) = hold_size(variant);
    let (hw, hd) = (w * 0.5, d * 0.5);
    let h = 3.3 + 1.2 * hash(k, 3);
    let lean = 0.5;
    let stone = Vec3::new(0.3, 0.275, 0.26) * (0.85 + 0.3 * hash(k, 4));
    let iron = Vec3::new(0.07, 0.07, 0.08);
    // The wall's face at height y, on the front (z) and the sides (x).
    let slope = |y: f32| lean * ((y - 0.5) / (h - 0.5)).clamp(0.0, 1.0);
    f.surface = BLOCKS;
    f.frustum(
        Vec3::ZERO,
        -0.7,
        (hw + 0.4, hd + 0.4),
        0.5,
        (hw + 0.3, hd + 0.3),
        (stone * 0.8, 0.0),
    )?;
    f.frustum(
        Vec3::ZERO,
        0.5,
        (hw, hd),
        h,
        (hw - lean, hd - lean),
        (stone, 0.0),
    )?;
    let eave = (hw - lean + 0.24, hd - lean + 0.24);
    f.frustum(Vec3::ZERO, h, eave, h + 0.36, eave, (stone * 0.9, 0.0))?;
    // The vault, along the hold's length.
    let rise = 1.5 + 0.8 * hash(k, 5);
    let base = h + 0.36;
    let (half, deep) = (eave.0 - 0.1, eave.1 - 0.1);
    let n = fine(detail, 18, 10, 5);
    let arc_at = |x: f32, j: usize, off: f32| {
        let a = j as f32 / n as f32 * PI;
        Vec3::new(x, base + (rise + off) * a.sin(), (deep + off) * a.cos())
    };
    f.surface = IRON;
    let mut rings = Vec::new();
    for x in [-half, half] {
        let mut ring = Vec::new();
        for j in 0..=n {
            let a = j as f32 / n as f32 * PI;
            let normal = Vec3::new(0.0, a.sin() * deep, a.cos() * rise);
            ring.push(f.point(arc_at(x, j, 0.0), normal, (iron * 1.6, 0.0))?);
        }
        rings.push(ring);
    }
    for j in 0..n {
        f.tri(rings[0][j], rings[0][j + 1], rings[1][j + 1])?;
        f.tri(rings[0][j], rings[1][j + 1], rings[1][j])?;
    }
    f.surface = BLOCKS;
    for x in [-half, half] {
        let gable: Vec<Vec3> = (0..=n).map(|j| arc_at(x, j, 0.0)).collect();
        f.face(&gable, Vec3::new(0.0, base, 0.0), (stone * 0.95, 0.0))?;
    }
    if solid(detail) {
        return Ok(f);
    }
    // Buttresses at the corners, and in the middle of a long wall.
    let mut feet = vec![(-hw, -hd), (hw, -hd), (hw, hd), (-hw, hd)];
    if w > 10.5 {
        feet.push((0.0, -hd));
    }
    for (x, z) in feet {
        let push = Vec3::new(x.signum() * 0.3, 0.0, z.signum() * 0.3);
        let top = h * 0.78;
        let low = Vec3::new(x, 0.0, z) + push;
        let high = Vec3::new(
            x - x.signum() * slope(top),
            0.0,
            z - z.signum() * slope(top),
        );
        let ring = |c: Vec3, y: f32, s: f32| {
            [
                c + Vec3::new(-s, y, -s),
                c + Vec3::new(s, y, -s),
                c + Vec3::new(s, y, s),
                c + Vec3::new(-s, y, s),
            ]
        };
        f.block(
            ring(low, -0.6, 0.75),
            ring(high, top, 0.36),
            (stone * 0.88, 0.0),
        )?;
    }
    // Iron bands over the vault.
    if detail < 2 {
        f.surface = IRON;
        let bands = (half * 2.0 / 1.7).round().max(2.0) as usize;
        for b in 0..=bands {
            let x = -half + 0.25 + (half * 2.0 - 0.5) * b as f32 / bands as f32;
            let path: Vec<Vec3> = (0..=n).map(|j| arc_at(x, j, 0.02)).collect();
            f.tube(&path, fine(detail, 6, 4, 3), &|_| 0.075, &|_| (iron, 0.0))?;
        }
    }
    // The porch: two piers and a round arch, the door set back in it.
    let front = hd + 0.3;
    let door_x = if w > 10.0 { -w * 0.16 } else { 0.0 };
    f.surface = BLOCKS;
    for side in [-1.0_f32, 1.0] {
        f.cuboid(
            Vec3::new(door_x + side * 0.98, 0.95, front),
            UP,
            Vec3::new(0.3, 1.25, 0.36),
            (stone * 0.82, 0.0),
        )?;
    }
    let arch: Vec<Vec3> = (0..=fine(detail, 12, 6, 3))
        .map(|j| {
            let a = j as f32 / fine(detail, 12, 6, 3) as f32 * PI;
            Vec3::new(door_x - 0.98 * a.cos(), 2.2 + 0.98 * a.sin(), front)
        })
        .collect();
    f.tube(&arch, fine(detail, 8, 5, 4), &|_| 0.32, &|_| {
        (stone * 0.82, 0.0)
    })?;
    f.surface = WOOD;
    let wood = Vec3::new(0.13, 0.08, 0.05);
    f.cuboid(
        Vec3::new(door_x, 1.1, hd - 0.1),
        UP,
        Vec3::new(0.72, 1.12, 0.32),
        (wood, 0.0),
    )?;
    let mut top: Vec<Vec3> = (0..=8)
        .map(|j| {
            let a = j as f32 / 8.0 * PI;
            Vec3::new(door_x - 0.72 * a.cos(), 2.2 + 0.72 * a.sin(), hd + 0.22)
        })
        .collect();
    top.reverse();
    f.face(&top, Vec3::new(door_x, 2.0, 0.0), (wood, 0.0))?;
    if detail == 0 {
        f.surface = IRON;
        for y in [0.5, 1.3, 2.05] {
            f.cuboid(
                Vec3::new(door_x, y, hd + 0.23),
                UP,
                Vec3::new(0.7, 0.06, 0.02),
                (iron, 0.0),
            )?;
        }
    }
    // Windows: small, deep in thick surrounds, barred.
    let light = (Vec3::new(1.0, 0.86, 0.6), 1.3);
    let window =
        |f: &mut Form, center: Vec3, right: Vec3, out: Vec3, lit: f32| -> EngineResult<()> {
            f.surface = PANE;
            let (ww, wh) = (0.3, 0.46);
            f.face(
                &[
                    center - right * ww - Vec3::Y * wh,
                    center + right * ww - Vec3::Y * wh,
                    center + right * ww + Vec3::Y * wh,
                    center - right * ww + Vec3::Y * wh,
                ],
                center - out,
                (light.0, light.1 * lit),
            )?;
            if detail == 2 {
                return Ok(());
            }
            f.surface = BLOCKS;
            let frame = [right, Vec3::Y, out];
            let c = center + out * 0.06;
            f.cuboid(
                c - Vec3::Y * (wh + 0.09),
                frame,
                Vec3::new(ww + 0.22, 0.09, 0.2),
                (stone * 0.8, 0.0),
            )?;
            f.cuboid(
                c + Vec3::Y * (wh + 0.13),
                frame,
                Vec3::new(ww + 0.26, 0.13, 0.2),
                (stone * 0.8, 0.0),
            )?;
            for side in [-1.0_f32, 1.0] {
                f.cuboid(
                    c + right * side * (ww + 0.1),
                    frame,
                    Vec3::new(0.1, wh, 0.17),
                    (stone * 0.8, 0.0),
                )?;
            }
            if detail == 0 {
                f.surface = IRON;
                for bar in [-0.1_f32, 0.1] {
                    let foot = center + right * bar + out * 0.1;
                    f.tube(
                        &[foot - Vec3::Y * wh, foot, foot + Vec3::Y * wh],
                        5,
                        &|_| 0.016,
                        &|_| (iron, 0.0),
                    )?;
                }
            }
            Ok(())
        };
    let sill = 1.95;
    let face = hd - slope(sill) + 0.03;
    let mut xs = vec![hw * 0.62, -hw * 0.62];
    if w > 10.0 {
        xs.push(hw * 0.2);
    }
    for (i, x) in xs.into_iter().enumerate() {
        if (x - door_x).abs() > 1.7 {
            window(
                &mut f,
                Vec3::new(x, sill, face),
                Vec3::X,
                Vec3::Z,
                0.5 + 0.6 * hash(k + i as u32, 6),
            )?;
        }
        window(
            &mut f,
            Vec3::new(x, sill, -face),
            -Vec3::X,
            -Vec3::Z,
            0.4 + 0.6 * hash(k + i as u32, 7),
        )?;
    }
    let side_face = hw - slope(sill) + 0.03;
    for s in [-1.0_f32, 1.0] {
        window(
            &mut f,
            Vec3::new(s * side_face, sill, 0.0),
            Vec3::Z * -s,
            Vec3::X * s,
            0.5 + 0.5 * hash(k, 8),
        )?;
    }
    // The stack, with the forge's glow in its mouth.
    let stack = Vec3::new(half * 0.55, base + rise * 0.55, -deep * 0.25);
    f.surface = BLOCKS;
    f.frustum(
        stack,
        0.0,
        (0.62, 0.62),
        1.9,
        (0.46, 0.46),
        (stone * 0.75, 0.0),
    )?;
    f.frustum(
        stack,
        1.9,
        (0.56, 0.56),
        2.12,
        (0.56, 0.56),
        (stone * 0.7, 0.0),
    )?;
    f.surface = PANE;
    let m = stack + Vec3::Y * 2.13;
    f.face(
        &[
            m + Vec3::new(-0.3, 0.0, -0.3),
            m + Vec3::new(0.3, 0.0, -0.3),
            m + Vec3::new(0.3, 0.0, 0.3),
            m + Vec3::new(-0.3, 0.0, 0.3),
        ],
        stack,
        (Vec3::new(1.0, 0.35, 0.12), 1.2),
    )?;
    Ok(f)
}

/// A goblin shack: a deck on crooked poles, walls and roof patched from
/// plates, planks and cloth, braced and lashed, with a ladder, a crooked
/// pipe and a lantern.
fn shack(variant: u32, detail: u32) -> EngineResult<Form> {
    let mut f = Form::new();
    let k = (variant + 1) * 877;
    let (hw, hd) = (1.5 + 0.6 * hash(k, 1), 1.4 + 0.5 * hash(k, 2));
    let deck = 1.2 + 0.9 * hash(k, 3);
    let wall = 1.9 + 0.3 * hash(k, 4);
    let woods = [
        Vec3::new(0.2, 0.13, 0.08),
        Vec3::new(0.27, 0.2, 0.13),
        Vec3::new(0.12, 0.09, 0.07),
    ];
    let rust = Vec3::new(0.26, 0.12, 0.06);
    let cloths = [
        Vec3::new(0.24, 0.06, 0.07),
        Vec3::new(0.05, 0.15, 0.15),
        Vec3::new(0.25, 0.2, 0.07),
        Vec3::new(0.15, 0.07, 0.2),
    ];
    // A patch: a thin plate of whatever was to hand, cut by eye, so no
    // corner is square and no edge runs true.
    let patch = |f: &mut Form, center: Vec3, right: Vec3, up: Vec3, half: (f32, f32), key: u32| {
        let out = right.cross(up);
        let stuff = hash(key, 2);
        let (surface, color) = if stuff < 0.45 {
            (WOOD, woods[(hash(key, 3) * 3.0) as usize % 3])
        } else if stuff < 0.82 {
            (IRON, rust.lerp(Vec3::new(0.12, 0.12, 0.13), hash(key, 3)))
        } else {
            (CLOTH, cloths[(hash(key, 3) * 4.0) as usize % 4])
        };
        f.surface = surface;
        let center = center + out * (hash(key, 4) - 0.5) * 0.1;
        let lean = out * (hash(key, 5) - 0.5) * 0.12;
        let corner = |sx: f32, sy: f32, n: u32| {
            center
                + right * sx * half.0 * (0.82 + 0.3 * hash(key, 10 + n))
                + up * sy * half.1 * (0.82 + 0.3 * hash(key, 20 + n))
                + lean * sy
        };
        let quad = [
            corner(-1.0, -1.0, 0),
            corner(1.0, -1.0, 1),
            corner(1.0, 1.0, 2),
            corner(-1.0, 1.0, 3),
        ];
        let back = out * 0.03;
        f.block(quad.map(|p| p - back), quad.map(|p| p + back), (color, 0.0))
    };
    // Poles: the four corners and one more, none straight.
    let mut corners = vec![(-hw, -hd), (hw, -hd), (hw, hd), (-hw, hd), (0.2 * hw, -hd)];
    if solid(detail) {
        corners.truncate(4);
    }
    f.surface = WOOD;
    for (i, &(x, z)) in corners.iter().enumerate() {
        let kk = k + 20 + i as u32;
        let top = deck + wall + 0.9 * hash(kk, 1);
        let lean = Vec3::new(x.signum() * 0.25, 0.0, z.signum() * 0.25);
        let bend = Vec3::new(hash(kk, 2) - 0.5, 0.0, hash(kk, 3) - 0.5) * 0.25;
        f.tube(
            &[
                Vec3::new(x, -0.7, z) + lean,
                Vec3::new(x, deck * 0.5, z) + lean * 0.5 + bend,
                Vec3::new(x, deck, z),
                Vec3::new(x, deck + wall * 0.6, z) - bend * 0.5,
                Vec3::new(x, top, z),
            ],
            fine(detail, 10, 6, 4),
            &|t| 0.1 - 0.035 * t,
            &|_| (woods[i % 3], 0.0),
        )?;
    }
    // The deck: planks, with gaps.
    let planks = fine(detail, 7, 5, 3);
    for p in 0..planks {
        let z = -hd + (p as f32 + 0.5) / planks as f32 * 2.0 * hd;
        let kk = k + 40 + p as u32;
        let yaw = (hash(kk, 1) - 0.5) * 0.06;
        f.cuboid(
            Vec3::new((hash(kk, 2) - 0.5) * 0.3, deck, z),
            [
                Vec3::new(yaw.cos(), 0.0, yaw.sin()),
                Vec3::Y,
                Vec3::new(-yaw.sin(), 0.0, yaw.cos()),
            ],
            Vec3::new(hw + 0.25, 0.035, hd / planks as f32 * 0.9),
            (woods[p % 3], 0.0),
        )?;
    }
    // Walls: back and sides patched shut, the front half open.
    let mid = deck + wall * 0.5;
    let across = fine(detail, 4, 3, 2);
    for (wall_n, (center, right)) in [
        (Vec3::new(0.0, mid, -hd), -Vec3::X),
        (Vec3::new(-hw, mid, 0.0), Vec3::Z),
        (Vec3::new(hw, mid, 0.0), -Vec3::Z),
        (Vec3::new(hw * 0.45, mid, hd), Vec3::X),
    ]
    .into_iter()
    .enumerate()
    {
        let span = if wall_n == 0 {
            hw
        } else if wall_n == 3 {
            hw * 0.5
        } else {
            hd
        };
        for a in 0..across {
            for tier in 0..2 {
                let kk = k + 60 + (wall_n * 20 + a * 2 + tier) as u32;
                let off = (a as f32 + 0.5 + 0.3 * (hash(kk, 7) - 0.5)) / across as f32 * 2.0 - 1.0;
                let tall = wall * (0.27 + 0.08 * hash(kk, 5));
                patch(
                    &mut f,
                    center + right * off * span + Vec3::Y * (tier as f32 - 0.5) * wall * 0.5,
                    right,
                    Vec3::Y,
                    (span / across as f32 * 1.15, tall),
                    kk,
                )?;
            }
        }
    }
    // The roof: plates lapped over each other, pitched to the back.
    let pitch = 0.25 + 0.2 * hash(k, 5);
    let slope_up = Vec3::new(0.0, pitch.sin(), pitch.cos());
    let plates = fine(detail, 5, 4, 2);
    for p in 0..plates {
        for row in 0..2 {
            let kk = k + 90 + (p * 2 + row) as u32;
            let x = -hw + (p as f32 + 0.5 + 0.3 * (hash(kk, 7) - 0.5)) / plates as f32 * 2.0 * hw;
            let along = (row as f32 - 0.5) * (hd + 0.4);
            patch(
                &mut f,
                Vec3::new(
                    x,
                    deck + wall + 0.14 + 0.04 * p as f32 + 0.05 * row as f32,
                    0.0,
                ) + slope_up * along,
                Vec3::X,
                slope_up,
                (hw / plates as f32 * 1.35, hd * 0.62 + 0.3),
                kk,
            )?;
        }
    }
    if solid(detail) || detail == 2 {
        return Ok(f);
    }
    // Braces from the poles' feet up under the deck.
    f.surface = WOOD;
    for (i, &(x, z)) in corners.iter().take(4).enumerate() {
        let to = corners[(i + 1) % 4];
        f.tube(
            &[
                Vec3::new(x, 0.1, z),
                Vec3::new((x + to.0) * 0.5, deck - 0.05, (z + to.1) * 0.5),
            ],
            fine(detail, 7, 5, 3),
            &|_| 0.05,
            &|_| (woods[(i + 1) % 3], 0.0),
        )?;
    }
    // The ladder.
    for x in [-0.28_f32, 0.28] {
        let x = x - hw * 0.45;
        f.tube(
            &[
                Vec3::new(x, -0.3, hd + 1.1),
                Vec3::new(x, deck + 0.5, hd + 0.2),
            ],
            fine(detail, 6, 4, 3),
            &|_| 0.035,
            &|_| (woods[1], 0.0),
        )?;
    }
    for r in 0..5 {
        let t = (r as f32 + 0.6) / 5.5;
        let p = Vec3::new(-hw * 0.45, -0.3 + (deck + 0.5) * t, hd + 1.1 - 0.9 * t);
        f.tube(
            &[p - Vec3::X * 0.3, p + Vec3::X * 0.3],
            5,
            &|_| 0.025,
            &|_| (woods[2], 0.0),
        )?;
    }
    // The pipe, with its hat.
    f.surface = IRON;
    let pipe = Vec3::new(hw * 0.4, deck + wall + 0.2, -hd * 0.3);
    f.tube(
        &[
            pipe,
            pipe + Vec3::new(0.05, 0.6, 0.0),
            pipe + Vec3::new(0.3, 0.9, 0.1),
            pipe + Vec3::new(0.3, 1.5, 0.1),
        ],
        fine(detail, 10, 6, 4),
        &|_| 0.09,
        &|_| (Vec3::new(0.1, 0.09, 0.09), 0.0),
    )?;
    f.lathe(
        pipe + Vec3::new(0.3, 1.5, 0.1),
        Vec3::Y,
        &[(0.0, 0.34), (0.22, 0.1), (0.2, 0.06), (0.0, 0.1)],
        fine(detail, 10, 6, 4),
        &|_, _| 1.0,
        &|_, _| (rust, 0.0),
    )?;
    // The light inside, seen through the open front, and a lantern out
    // on a pole.
    let light = (Vec3::new(1.0, 0.36, 0.5), 1.3);
    f.surface = PANE;
    f.drop(
        Vec3::new(-hw * 0.3, deck + wall * 0.85, 0.0),
        -Vec3::Y,
        0.12,
        0.3,
        fine(detail, 12, 7, 4),
        light,
    )?;
    let hook = Vec3::new(-hw - 0.5, deck + wall + 0.3, hd + 0.5);
    f.surface = WOOD;
    f.tube(
        &arc(
            Vec3::new(-hw, deck + 0.2, hd),
            Vec3::new(-0.2, 0.0, 0.2),
            wall + 0.3,
            0.3,
            4,
        ),
        fine(detail, 6, 4, 3),
        &|_| 0.03,
        &|_| (woods[0], 0.0),
    )?;
    f.surface = PANE;
    f.drop(
        hook - Vec3::Y * 0.2,
        -Vec3::Y,
        0.1,
        0.28,
        fine(detail, 12, 7, 4),
        light,
    )?;
    // Lashings where pole meets deck.
    if detail == 0 {
        f.surface = CLOTH;
        for &(x, z) in corners.iter().take(4) {
            f.lathe(
                Vec3::new(x, deck - 0.08, z),
                Vec3::Y,
                &[
                    (0.1, 0.0),
                    (0.135, 0.03),
                    (0.14, 0.08),
                    (0.135, 0.13),
                    (0.1, 0.16),
                ],
                10,
                &|_, _| 1.0,
                &|ring, _| (Vec3::new(0.4, 0.33, 0.2) * (0.7 + 0.15 * ring as f32), 0.0),
            )?;
        }
    }
    Ok(f)
}

/// An umbral street lamp: a thin post that curls over, a lit drop
/// hanging from it.
fn lamp(_: u32, detail: u32) -> EngineResult<Form> {
    let mut f = Form::new();
    let metal = Vec3::new(0.1, 0.19, 0.16);
    f.surface = LATTICE;
    let n = fine(detail, 16, 8, 4);
    let path: Vec<Vec3> = (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            // Straight up, then over in a crook.
            let a = smooth(0.6, 1.0, t) * 2.4;
            Vec3::new(
                0.0,
                -0.4 + 3.6 * t.min(0.7) / 0.7 + 0.45 * a.sin(),
                0.45 * (1.0 - a.cos()),
            )
        })
        .collect();
    f.tube(&path, fine(detail, 8, 5, 3), &|t| 0.055 - 0.03 * t, &|_| {
        (metal, 0.0)
    })?;
    if solid(detail) {
        return Ok(f);
    }
    let tip = path[n];
    f.surface = PANE;
    f.drop(
        tip,
        -Vec3::Y,
        0.14,
        0.4,
        fine(detail, 14, 8, 4),
        (Vec3::new(0.55, 1.0, 0.8), 2.0),
    )?;
    Ok(f)
}

/// A dwarven brazier: a short thick pillar of stone with an iron bowl of
/// coals.
fn brazier(_: u32, detail: u32) -> EngineResult<Form> {
    let mut f = Form::new();
    let stone = Vec3::new(0.28, 0.26, 0.25);
    f.surface = BLOCKS;
    f.frustum(
        Vec3::ZERO,
        -0.5,
        (0.42, 0.42),
        1.15,
        (0.3, 0.3),
        (stone, 0.0),
    )?;
    f.frustum(
        Vec3::ZERO,
        1.15,
        (0.38, 0.38),
        1.3,
        (0.38, 0.38),
        (stone * 0.85, 0.0),
    )?;
    if solid(detail) {
        return Ok(f);
    }
    f.surface = IRON;
    f.lathe(
        Vec3::Y * 1.3,
        Vec3::Y,
        &[
            (0.0, 0.0),
            (0.26, 0.03),
            (0.42, 0.2),
            (0.45, 0.3),
            (0.4, 0.3),
            (0.3, 0.16),
            (0.0, 0.14),
        ],
        fine(detail, 20, 10, 6),
        &|_, _| 1.0,
        &|_, _| (Vec3::new(0.07, 0.07, 0.08), 0.0),
    )?;
    f.surface = PANE;
    f.lathe(
        Vec3::Y * 1.46,
        Vec3::Y,
        &[(0.0, 0.14), (0.16, 0.1), (0.3, 0.02)],
        fine(detail, 14, 8, 5),
        &|ring, angle| 1.0 + 0.12 * (angle * 5.0 + ring as f32).cos(),
        &|_, angle| {
            let ember = 0.6 + 0.4 * (angle * 7.0).cos();
            (Vec3::new(1.0, 0.33, 0.1) * ember, 1.6 * ember)
        },
    )?;
    Ok(f)
}

type Job = Box<dyn FnOnce() -> EngineResult<Stand> + Send>;
/// Makes a thing's shape from which shape of it and a level of detail.
type Shaper = fn(u32, u32) -> EngineResult<Form>;

/// A building's shape cut into the pieces it breaks into (W1): what each
/// kind of surface is bonded like, and how thick its masonry is.
fn shards(make: Make, form: &Form) -> EngineResult<Vec<Shard>> {
    use BondClass::{Dwarven, Light, Masonry, Timber};
    let is = |kind: f32, what: f32| (kind - what).abs() < 0.5;
    if make.loose() {
        return Ok(Vec::new());
    }
    let (cells, hold): (Cells, Hold) = match make {
        Make::Tower => (Cells::Round(2.4, 6), Hold::Foot),
        Make::Bridge => (Cells::Square(Vec3::new(50.0, 50.0, 2.5)), Hold::Ends),
        Make::Hold => (Cells::Square(Vec3::new(2.2, 1.7, 2.2)), Hold::Foot),
        Make::Shack => (Cells::Square(Vec3::new(1.7, 1.5, 1.7)), Hold::Foot),
        _ => (Cells::One, Hold::Foot),
    };
    shatter(form, cells, hold, &|kind| match make {
        // Umbral work: light foamed walls in a lattice of metal.
        Make::Tower if is(kind, FOAM) => (Timber, Some(0.3)),
        Make::Tower if is(kind, LATTICE) => (Masonry, None),
        Make::Tower | Make::Bridge if is(kind, PANE) => (Light, None),
        Make::Tower | Make::Bridge => (Timber, None),
        // Dwarven heavy masonry and iron.
        Make::Hold if is(kind, BLOCKS) => (Dwarven, Some(0.45)),
        Make::Hold if is(kind, IRON) => (Dwarven, None),
        Make::Hold if is(kind, PANE) => (Light, None),
        Make::Hold => (Timber, None),
        // Goblin patchwork: poles hold a little, the rest hardly.
        Make::Shack if is(kind, WOOD) => (Timber, None),
        _ => (Light, None),
    })
}

/// The buildings as jobs for worker threads: one per thing and shape.
pub fn stands(sites: &[Site]) -> Vec<Job> {
    let mut jobs: Vec<Job> = Vec::new();
    for make in MAKES {
        for variant in 0..variants(make) {
            let places: Vec<Place> = sites
                .iter()
                .filter(|s| s.make == make && s.variant == variant)
                .map(|s| space::place(s.at, s.rot))
                .collect();
            if places.is_empty() {
                continue;
            }
            jobs.push(Box::new(move || {
                let (shape, reach): (Shaper, [f32; 3]) = match make {
                    Make::Tower => (tower, [110.0, 420.0, 6000.0]),
                    Make::Bridge => (bridge, [80.0, 300.0, 3000.0]),
                    Make::Hold => (hold, [90.0, 360.0, 5000.0]),
                    Make::Shack => (shack, [60.0, 220.0, 2500.0]),
                    Make::Lamp => (lamp, [40.0, 140.0, 1200.0]),
                    Make::Brazier => (brazier, [40.0, 140.0, 1200.0]),
                    Make::Jar => (crate::goods::jar, [16.0, 55.0, 320.0]),
                    Make::Cask => (crate::goods::cask, [16.0, 55.0, 320.0]),
                    Make::Crate => (crate::goods::crate_box, [16.0, 55.0, 320.0]),
                    Make::Sack => (crate::goods::sack, [16.0, 55.0, 320.0]),
                };
                let mut levels = Vec::new();
                for (detail, to) in reach.into_iter().enumerate() {
                    levels.push((shape(variant, detail as u32)?.mesh.prepare(), to));
                }
                Ok(Stand {
                    levels,
                    places,
                    shadow: true,
                    stuff: Stuff::Built,
                    solid: Some(shape(variant, 3)?.mesh),
                    pieces: shards(make, &shape(variant, 0)?)?,
                    loose: if make.loose() {
                        Some(shape(variant, 0)?.mesh)
                    } else {
                        None
                    },
                })
            }));
        }
    }
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_building_builds_at_every_level_and_as_a_solid() {
        for make in MAKES {
            let shape: Shaper = match make {
                Make::Tower => tower,
                Make::Bridge => bridge,
                Make::Hold => hold,
                Make::Shack => shack,
                Make::Lamp => lamp,
                Make::Brazier => brazier,
                Make::Jar => crate::goods::jar,
                Make::Cask => crate::goods::cask,
                Make::Crate => crate::goods::crate_box,
                Make::Sack => crate::goods::sack,
            };
            for variant in 0..variants(make) {
                let mut last = usize::MAX;
                for detail in 0..3 {
                    let n = shape(variant, detail).unwrap().mesh.face_count();
                    assert!(
                        n > 0 && n < last,
                        "{make:?} {variant} level {detail}: {n} after {last}"
                    );
                    last = n;
                }
                assert!(shape(variant, 3).unwrap().mesh.face_count() > 0);
            }
        }
    }

    #[test]
    fn every_building_comes_apart_into_pieces_that_hang_together() {
        for make in MAKES {
            let shape: Shaper = match make {
                Make::Tower => tower,
                Make::Bridge => bridge,
                Make::Hold => hold,
                Make::Shack => shack,
                Make::Lamp => lamp,
                Make::Brazier => brazier,
                Make::Jar => crate::goods::jar,
                Make::Cask => crate::goods::cask,
                Make::Crate => crate::goods::crate_box,
                Make::Sack => crate::goods::sack,
            };
            if make.loose() {
                continue;
            }
            for variant in 0..variants(make) {
                let form = shape(variant, 0).unwrap();
                let pieces = shards(make, &form).unwrap();
                let faces: usize = pieces.iter().map(|p| p.mesh.face_count()).sum();
                eprintln!(
                    "{make:?} {variant}: {} pieces, {faces} faces from {}",
                    pieces.len(),
                    form.tris.len()
                );
                assert!(!pieces.is_empty() && pieces.len() < 700);
                assert!(faces >= form.tris.len());
                // Every piece is held, through its neighbours, by an anchor.
                let mut held: Vec<bool> = pieces.iter().map(|p| p.anchored).collect();
                let mut queue: Vec<usize> = (0..pieces.len()).filter(|&i| held[i]).collect();
                assert!(!queue.is_empty(), "{make:?} {variant} has no anchor");
                while let Some(i) = queue.pop() {
                    for &j in &pieces[i].neighbours {
                        if !held[j] {
                            held[j] = true;
                            queue.push(j);
                        }
                    }
                }
                let loose = held.iter().filter(|h| !**h).count();
                assert_eq!(
                    loose, 0,
                    "{make:?} {variant}: {loose} pieces hang in the air"
                );
            }
        }
    }

    #[test]
    fn the_town_has_towers_holds_and_shacks() {
        let rock = rock::shared();
        let sites = plan(rock, &rock.waters(space::HEART_CENTER.normalize()));
        for (make, least) in [
            (Make::Tower, 20),
            (Make::Hold, 12),
            (Make::Shack, 8),
            (Make::Bridge, 5),
        ] {
            let n = sites.iter().filter(|s| s.make == make).count();
            eprintln!("{make:?}: {n}");
            assert!(n >= least, "{make:?}: {n}");
        }
    }
}
