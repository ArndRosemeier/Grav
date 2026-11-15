//! The look of the world: the dead stone and lodestone materials, the heart
//! as a light in the sky of the cavern, and the light and air around it
//! (design doc "Look and style", L1 to L13, and "Quality bar" Q5).
//!
//! The materials are WGSL run inside the engine's lit shader, so they are
//! lit, shadowed and hazed like everything else. They work from the world
//! position, so every piece of rock continues the same strata and veins.

use crate::heart;
use crate::rock::RockKind;
use engine::prelude::*;

/// Direction from the town to the heart (unit). The spire grows inward and a
/// little under the heart, so its light rakes along the street.
pub fn heart_direction(heart_center: Vec3) -> Vec3 {
    heart_center.normalize()
}

/// The heart's light: cold blue-white, the colour of repelling lodestone (L1).
const HEART_LIGHT: Vec3 = Vec3::new(0.72, 0.84, 1.0);
/// How far away the heart's stand-in body is drawn.
const HEART_SHOWN_AT: f32 = 1700.0;
/// The radius of the heart's core there. With its spines the body spans
/// far more of the sky than one view holds.
const HEART_CORE: f32 = 450.0;

/// Shared WGSL: hash, value noise, fbm.
const NOISE: &str = r#"
fn gv_hash(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q = q + dot(q, q.yxz + 33.33);
    return fract((q.x + q.y) * q.z);
}
fn gv_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(gv_hash(i), gv_hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let b = mix(gv_hash(i + vec3<f32>(0.0, 1.0, 0.0)), gv_hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let c = mix(gv_hash(i + vec3<f32>(0.0, 0.0, 1.0)), gv_hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let d = mix(gv_hash(i + vec3<f32>(0.0, 1.0, 1.0)), gv_hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}
fn gv_fbm3(p: vec3<f32>) -> f32 {
    return gv_noise(p) * 0.57 + gv_noise(p * 2.07 + 11.3) * 0.29 + gv_noise(p * 4.21 + 27.1) * 0.14;
}
fn gv_fbm5(p: vec3<f32>) -> f32 {
    return gv_noise(p) * 0.5 + gv_noise(p * 2.07 + 11.3) * 0.26 + gv_noise(p * 4.21 + 27.1) * 0.13
        + gv_noise(p * 8.4 + 5.7) * 0.07 + gv_noise(p * 17.1 + 41.9) * 0.04;
}
"#;

/// Dead stone (L7 to L10) with lodestone seams (L1 to L3).
///
/// params[0].xyz: direction to the heart. params[0].w: detail scale (1 near
/// rock, less for far scenery).
/// params[1].x: how much lodestone shows in seams (0 none .. 1 a vein
/// district). params[1].y: seam glow strength.
const DEAD_STONE: &str = r#"
// A crease field: sharp cracks between rounded faces, 0 in a crack.
fn gv_crease(p: vec3<f32>) -> f32 {
    return abs(gv_noise(p) * 2.0 - 1.0);
}

// The small relief of the surface, as a height in 0..1: cracked faces at
// three sizes, then grain.
fn rock_relief(p: vec3<f32>, layer: f32) -> f32 {
    let q = p + (gv_noise(p * 1.1 + 50.0) - 0.5) * 0.5;
    let faces = gv_crease(q * 2.3) * 0.30 + gv_crease(q * 5.7 + 13.0) * 0.26
        + gv_crease(q * 13.0 + 29.0) * 0.16;
    let grain = gv_noise(p * 31.0) * 0.07 + gv_noise(p * 67.0) * 0.05;
    // Each stratum weathers its own way: hard layers stand proud.
    return faces + grain + layer * 0.16;
}

// One family of metal threads: thin parallel curves, as if a field combed them.
fn metal_threads(p: vec3<f32>, dir: vec3<f32>, bend: vec3<f32>, spacing: f32, width: f32, warp: f32) -> f32 {
    let c = dot(p, dir) / spacing + sin(dot(p, bend) * 0.045) * 2.6 + warp * 3.0
        + (gv_fbm3(p * 0.19 + 17.0) - 0.5) * 2.4 + (gv_noise(p * 1.3) - 0.5) * 0.22;
    let f = abs(fract(c) - 0.5);
    // Threads come and go along their length.
    let along = gv_noise(vec3<f32>(floor(c) * 7.3, dot(p, bend) * 0.11, 3.0));
    // Threads run in bundles with bare rock between.
    let bundle = smoothstep(0.5, 0.62, gv_noise(vec3<f32>(c * 0.23, 1.0, 5.0)));
    let w = width * (0.5 + gv_hash(vec3<f32>(floor(c), 2.0, 8.0)));
    return (1.0 - smoothstep(w * 0.5, w, f)) * smoothstep(0.42, 0.6, along) * bundle;
}

fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let p = m.world_pos;
    let heart = normalize(m.params[0].xyz);
    let detail = m.params[0].w;
    let n0 = normalize(m.normal);

    // Strata: with no single down the layers curl and fold (L9).
    let warp = gv_fbm3(p * 0.021);
    let fold = gv_fbm3(p * 0.006 + 40.0);
    let s = dot(p, vec3<f32>(0.24, 0.93, 0.28)) * 0.55 + fold * 46.0 + warp * 7.0;
    let layer_id = floor(s);
    let layer_f = fract(s);
    let layer_tone = gv_hash(vec3<f32>(layer_id, 3.0, 9.0));
    let layer_hard = gv_hash(vec3<f32>(layer_id, 17.0, 1.0));
    let seam_line = smoothstep(0.0, 0.05, layer_f) * (1.0 - smoothstep(0.95, 1.0, layer_f));

    // Base colour: dark, matt, a little violet-grey (L7).
    let dark = vec3<f32>(0.022, 0.021, 0.027);
    let mid = vec3<f32>(0.085, 0.080, 0.090);
    let warm = vec3<f32>(0.105, 0.082, 0.064);
    var color = mix(dark, mid, layer_tone * layer_tone);
    color = mix(color, warm, smoothstep(0.6, 0.9, gv_hash(vec3<f32>(layer_id, 5.0, 2.0))) * 0.7);
    let mottle = gv_fbm5(p * 0.33 + 9.0);
    color = color * (0.6 + 0.8 * mottle) * (0.4 + 0.6 * seam_line);
    var rough = 1.0 - layer_hard * 0.05;
    var metal = 0.0;

    // Relief: bump the normal from the height's slope (three more samples).
    let e = 0.012 / max(detail, 0.25);
    let h0 = rock_relief(p, layer_hard * seam_line);
    let hx = rock_relief(p + vec3<f32>(e, 0.0, 0.0), layer_hard * seam_line);
    let hy = rock_relief(p + vec3<f32>(0.0, e, 0.0), layer_hard * seam_line);
    let hz = rock_relief(p + vec3<f32>(0.0, 0.0, e), layer_hard * seam_line);
    let grad = vec3<f32>(hx - h0, hy - h0, hz - h0) / e;
    let bump = (grad - n0 * dot(grad, n0)) * 0.05 * detail;
    var n = normalize(n0 - bump);
    // Crevices are darker.
    color = color * (0.45 + 0.75 * smoothstep(0.12, 0.6, h0));
    // Mineral grains: fine speckle, a few bright flakes.
    let grain = gv_noise(p * 37.0);
    color = color * (0.86 + 0.28 * grain);
    let flake = smoothstep(0.86, 0.93, gv_noise(p * 61.0 + 3.0)) * detail;
    color = color + vec3<f32>(0.05, 0.05, 0.055) * flake;
    rough = rough - 0.15 * flake;

    // Metal threads combed by old fields (L8): copper, iron, a pale silvery metal.
    let province = gv_fbm3(p * 0.013 + 70.0);
    let copper = metal_threads(p, vec3<f32>(0.62, 0.31, 0.72), vec3<f32>(-0.4, 0.8, 0.45), 0.9, 0.055, warp)
        * smoothstep(0.52, 0.62, province);
    let iron = metal_threads(p, vec3<f32>(-0.35, 0.74, 0.57), vec3<f32>(0.8, 0.1, -0.6), 1.7, 0.09, warp)
        * smoothstep(0.50, 0.38, province);
    let silver = metal_threads(p, vec3<f32>(0.2, -0.55, 0.81), vec3<f32>(0.6, 0.7, 0.4), 2.6, 0.035, warp)
        * smoothstep(0.42, 0.5, gv_fbm3(p * 0.017 + 123.0));
    let threads = clamp(copper + iron + silver, 0.0, 1.0);
    let metal_color = (vec3<f32>(0.86, 0.42, 0.24) * copper + vec3<f32>(0.30, 0.28, 0.27) * iron
        + vec3<f32>(0.82, 0.84, 0.88) * silver) / max(copper + iron + silver, 0.001);
    color = mix(color, metal_color, threads);
    metal = threads;
    rough = mix(rough, 0.28 + 0.2 * gv_noise(p * 6.0), threads);
    n = normalize(mix(n, n0, threads * 0.6));

    // Heart-side weathering (L10): faces to the heart are bleached and glazed,
    // far sides dark and damp with pale mineral bloom.
    let facing = dot(n0, heart);
    let bleach = smoothstep(0.05, 0.85, facing) * (1.0 - threads);
    color = mix(color, color * 1.2 + vec3<f32>(0.004, 0.005, 0.007), bleach * 0.5);
    rough = mix(rough, 0.9 + 0.1 * mottle, bleach * 0.6);
    let shade = smoothstep(0.0, -0.7, facing) * (1.0 - threads);
    color = color * (1.0 - shade * 0.35);
    let damp = shade * smoothstep(0.45, 0.7, gv_fbm3(p * 0.09 + 5.0));
    rough = mix(rough, 0.45, damp * 0.8);
    let bloom_patch = shade * smoothstep(0.66, 0.8, gv_fbm5(p * 0.21 + 31.0)) * smoothstep(0.3, 0.6, h0);
    color = mix(color, vec3<f32>(0.20, 0.19, 0.13), bloom_patch * 0.8);
    rough = mix(rough, 0.95, bloom_patch);

    // Lodestone seams: where a vein comes to the surface the rock is cracked
    // and the stone shows, lit from inside in drifting field lines (L2, L3).
    let seams = m.params[1].x;
    var emission = vec3<f32>(0.0);
    if (seams > 0.0) {
        let district = smoothstep(0.62 - 0.3 * seams, 0.78 - 0.3 * seams, gv_fbm3(p * 0.011 + 200.0));
        let crack_field = gv_fbm3(p * 0.045 + 300.0) - 0.5 + (gv_noise(p * 0.4) - 0.5) * 0.05;
        let crack_w = 0.004 + 0.009 * gv_noise(p * 0.07 + 9.0);
        let crack = (1.0 - smoothstep(crack_w * 0.4, crack_w, abs(crack_field))) * district;
        let halo = (1.0 - smoothstep(0.0, crack_w * 5.0, abs(crack_field))) * district;
        // Field lines: level curves of a smooth field, drifting slowly.
        let lines_field = gv_fbm3(p * 0.55 + vec3<f32>(0.0, m.time * 0.02, 0.0));
        let lines = pow(0.5 + 0.5 * sin(lines_field * 30.0 + m.time * 0.35), 6.0);
        let embers = smoothstep(0.75, 0.95, gv_noise(p * 3.1 + vec3<f32>(m.time * 0.08)));
        let pulse = 0.75 + 0.25 * sin(m.time * 0.5 + gv_noise(p * 0.03) * 12.0);
        let amber = vec3<f32>(1.0, 0.36, 0.04);
        let glow = (0.12 + 3.2 * lines + 0.8 * embers * lines) * pulse * m.params[1].y;
        // The stone itself: banded like agate under the glow (L4).
        let agate = 0.5 + 0.5 * sin(lines_field * 15.0);
        color = mix(color, mix(vec3<f32>(0.035, 0.012, 0.004), vec3<f32>(0.16, 0.07, 0.015), agate), crack);
        rough = mix(rough, 0.35, crack);
        metal = mix(metal, 0.0, crack);
        emission = amber * (crack * glow + halo * halo * 0.05 * m.params[1].y);
    }

    // Rough stone has no mirror sheen at a glancing view: its grain hides
    // it. Lean the shading normal towards the eye there, so the sky's
    // reflection does not wash the strata out along a street.
    let to_eye = normalize(m.view_dir) * sign(dot(n0, m.view_dir));
    let glance = 1.0 - clamp(dot(n, to_eye), 0.0, 1.0);
    n = normalize(n + to_eye * glance * glance * 1.4 * (1.0 - metal));
    o.color = vec4<f32>(color, 1.0);
    o.normal = n;
    o.roughness = clamp(rough, 0.05, 1.0);
    o.metallic = clamp(metal, 0.0, 1.0);
    o.emission = emission;
    return o;
}
"#;

/// The materials and the heart's body, made once.
pub struct Look {
    spire: CustomMaterialId,
    wall: CustomMaterialId,
    island: CustomMaterialId,
    far: CustomMaterialId,
    heart: EntityId,
    heart_dir: Vec3,
}

impl Look {
    pub fn new(world: &mut World, heart_center: Vec3) -> EngineResult<Self> {
        let heart_dir = heart_direction(heart_center);
        let stone = |name: &str, detail: f32, seams: f32, glow: f32| {
            CustomMaterial::new(name, format!("{NOISE}{DEAD_STONE}"))
                .param(0, [heart_dir.x, heart_dir.y, heart_dir.z, detail])
                .param(1, [seams, glow, 0.0, 0.0])
        };
        // The spire and the island carry a vein, so their stone shows; the
        // wall's vein lies deep and shows in few places; far spires glow
        // enough to read as lived-in from a kilometre away.
        let spire = world.add_material(stone("dead stone, spire", 1.0, 0.6, 1.0))?;
        let wall = world.add_material(stone("dead stone, wall", 1.0, 0.35, 1.0))?;
        let island = world.add_material(stone("dead stone, island", 1.0, 0.6, 0.9))?;
        let far = world.add_material(stone("dead stone, far", 0.5, 0.7, 1.4))?;
        let blade_material = world.add_material(
            CustomMaterial::new("heart blades", format!("{NOISE}{}", heart::BLADES))
                .param(0, [9.0, HEART_CORE, 0.0, 0.0]),
        )?;
        let blades = world.spawn(heart::body()?);
        world.set_custom_material(blades, blade_material)?;
        let heart = blades;
        world.set_place(heart, heart_place(heart_dir * HEART_SHOWN_AT))?;
        world.set_casts_shadow(heart, false)?;
        Ok(Self {
            spire,
            wall,
            island,
            far,
            heart,
            heart_dir,
        })
    }

    pub fn rock_material(&self, kind: RockKind) -> CustomMaterialId {
        match kind {
            RockKind::Spire => self.spire,
            RockKind::Wall => self.wall,
            RockKind::Island => self.island,
            RockKind::FarSpire => self.far,
        }
    }

    /// Light and air: the heart as the one great light, deep cool shadow,
    /// mist that takes the heart's colour (Q5).
    pub fn light_and_air(&self, world: &mut World) -> EngineResult<()> {
        let mist = rgb(5, 8, 17);
        world.set_sky(Some(Sky {
            zenith: mist,
            horizon: mist,
            ground: mist,
            sun_color: rgb(20, 34, 72),
            sun_size_degrees: 6.0,
            sun_bloom_degrees: 38.0,
            curve: 0.0,
        }));
        world.set_physical_sky(None);
        world.set_clear_color(mist);
        world.set_sun(self.heart_dir, 0.10);
        world.set_sun_color(HEART_LIGHT * 1.9);
        let mut haze = Haze::new(mist, 2600.0);
        haze.base_y = -100_000.0;
        haze.height_m = 1_000_000.0;
        world.set_haze(Some(haze));
        world.set_view_distance(6000.0)?;
        world.set_shadows(Some(ShadowSettings {
            cascade_end_m: [18.0, 70.0, 260.0],
            map_size: 2048,
            raymarch_height: false,
            ..ShadowSettings::default()
        }));
        world.set_bloom(BloomSettings::default().with_intensity(0.55)?);
        world.set_tone_map(ToneMap::Agx);
        Ok(())
    }

    /// Every frame: the heart is so far away that it does not shift as the
    /// camera moves, so its stand-in keeps its distance from the camera.
    pub fn follow_camera(&self, world: &mut World, camera: Vec3) -> EngineResult<()> {
        world.set_place(
            self.heart,
            heart_place(camera + self.heart_dir * HEART_SHOWN_AT),
        )
    }
}

fn heart_place(p: Vec3) -> Place {
    Place::new(p.x, p.y, p.z).with_scale(HEART_CORE)
}
