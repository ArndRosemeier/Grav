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
/// The radius of the heart's core. Its spines and shed shards reach to
/// about 3.3 km, inside the 3 to 3.6 km the heart keeps stripped bare.
const HEART_CORE: f32 = 900.0;

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
// Broken rock: p falls in one of many irregular cells. Returns the cell's
// own number (0..1) in x and the distance to the cell's edge in y (0 on a
// joint between two cells).
fn gv_cells(p: vec3<f32>) -> vec2<f32> {
    let base = floor(p);
    var d1 = 9.0;
    var d2 = 9.0;
    var own = 0.0;
    for (var i = -1; i <= 1; i = i + 1) {
        for (var j = -1; j <= 1; j = j + 1) {
            for (var k = -1; k <= 1; k = k + 1) {
                let c = base + vec3<f32>(f32(i), f32(j), f32(k));
                let at = c + vec3<f32>(gv_hash(c + 1.7), gv_hash(c + 5.3), gv_hash(c + 9.1));
                let d = distance(at, p);
                if (d < d1) {
                    d2 = d1;
                    d1 = d;
                    own = gv_hash(c + 13.9);
                } else if (d < d2) {
                    d2 = d;
                }
            }
        }
    }
    return vec2<f32>(own, d2 - d1);
}
// Three numbers in 0..1 for a lattice point, for the price of one.
fn gv_hash3(p: vec3<f32>) -> vec3<f32> {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q = q + dot(q, q.yxz + 33.33);
    return fract((q.xxy + q.yxx) * q.zyx);
}
// gv_cells at about a third of the cost (one hash per cell instead of
// four), for rock that fills the far view.
fn gv_cells_far(p: vec3<f32>) -> vec2<f32> {
    let base = floor(p);
    var d1 = 9.0;
    var d2 = 9.0;
    var own = 0.0;
    for (var i = -1; i <= 1; i = i + 1) {
        for (var j = -1; j <= 1; j = j + 1) {
            for (var k = -1; k <= 1; k = k + 1) {
                let c = base + vec3<f32>(f32(i), f32(j), f32(k));
                let h = gv_hash3(c + 1.7);
                let d = distance(c + h, p);
                if (d < d1) {
                    d2 = d1;
                    d1 = d;
                    own = fract(h.x * 17.3 + h.y * 5.1 + h.z * 31.7);
                } else if (d < d2) {
                    d2 = d;
                }
            }
        }
    }
    return vec2<f32>(own, d2 - d1);
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
    // Nearly straight and parallel, as a comb leaves them: a slow lean, a
    // little unevenness, no snaking.
    let c = dot(p, dir) / spacing + dot(p, bend) * 0.012 + warp * 1.2
        + (gv_fbm3(p * 0.05 + 17.0) - 0.5) * 0.9 + (gv_noise(p * 1.3) - 0.5) * 0.1;
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
    // They are broken, not drawn with a pen: the rock is in blocks, each
    // slipped against its neighbours along joints, so layers end at a joint
    // and go on higher or lower beyond it. Beds differ in thickness, and
    // their edges are ragged.
    let warp = gv_fbm3(p * 0.021);
    let fold = gv_fbm3(p * 0.004 + 40.0);
    let blocks = gv_cells(p * vec3<f32>(0.085, 0.06, 0.075) + (warp - 0.5) * 0.5);
    let joint = 1.0 - smoothstep(0.0, 0.035, blocks.y);
    let slip = (blocks.x - 0.5) * 7.0;
    let ragged = (gv_noise(p * 2.1) - 0.5) * 0.35 + (gv_noise(p * 7.0) - 0.5) * 0.12;
    let s0 = dot(p, vec3<f32>(0.24, 0.93, 0.28)) * 0.42 + fold * 13.0 + warp * 2.0 + slip;
    // Beds of uneven thickness: thick and thin ones in runs.
    let s = s0 + gv_noise(vec3<f32>(s0 * 0.21, 2.0, 7.0)) * 2.5 + ragged;
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
    // Stains and weathering in patches as large as the beds, so the eye
    // reads rock first and stripes second.
    let patches = gv_fbm5(p * 0.045 + 61.0);
    color = mix(color, mid * (0.45 + 0.5 * blocks.x), 0.45 * smoothstep(0.35, 0.65, patches));
    color = color * (0.6 + 0.8 * mottle) * (0.55 + 0.45 * seam_line) * (1.0 - 0.55 * joint);
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

    // A road (uv.y): flags of the same stone laid close, each its own
    // tone, their middles worn smooth by feet, the joints dark and full
    // of metal dust that the pull has combed into them.
    let tread = smoothstep(0.3, 0.7, m.uv.y + (gv_fbm3(p * 0.8 + 19.0) - 0.5) * 0.5);
    if (tread > 0.01) {
        let lay = (gv_fbm3(p * 0.6 + 77.0) - 0.5) * 0.25;
        let flags = gv_cells(p * 0.95 + 500.0 + lay);
        let e2 = 0.02;
        let fx = gv_cells((p + vec3<f32>(e2, 0.0, 0.0)) * 0.95 + 500.0 + lay).y;
        let fy = gv_cells((p + vec3<f32>(0.0, e2, 0.0)) * 0.95 + 500.0 + lay).y;
        let fz = gv_cells((p + vec3<f32>(0.0, 0.0, e2)) * 0.95 + 500.0 + lay).y;
        let gap = 1.0 - smoothstep(0.012, 0.05, flags.y);
        let lip = 1.0 - smoothstep(0.0, 0.14, flags.y);
        let worn = smoothstep(0.08, 0.32, flags.y) * (0.55 + 0.45 * gv_noise(p * 2.3 + 5.0));
        var flag = mix(mid, warm, smoothstep(0.55, 0.9, flags.x) * 0.6) * (0.55 + 1.1 * flags.x);
        flag = flag * (0.7 + 0.6 * mottle) * (0.88 + 0.24 * grain) * (0.7 + 0.3 * smoothstep(0.1, 0.5, h0));
        // Worn middles are paler with dust and smoother.
        flag = mix(flag, flag * 1.25 + vec3<f32>(0.008), worn * 0.6);
        let filings = gap * smoothstep(0.5, 0.85, gv_noise(p * 47.0 + 3.0)) * (0.4 + 0.6 * gv_noise(p * 5.0));
        flag = mix(flag * (1.0 - 0.8 * gap), vec3<f32>(0.2, 0.18, 0.17), filings * 0.6);
        color = mix(color, flag, tread);
        rough = mix(rough, mix(mix(0.9, 0.52, worn), 0.4, filings), tread);
        metal = mix(metal, 0.85, filings * tread);
        // The flags' edges are rounded down into the joints.
        let fgrad = vec3<f32>(fx - flags.y, fy - flags.y, fz - flags.y) / e2;
        let slope = (fgrad - n0 * dot(fgrad, n0)) * lip * 0.12;
        n = normalize(mix(n, n0, tread * 0.75 * (1.0 - gap)) + slope * tread);
    }

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
    metal = max(metal * (1.0 - threads), threads);
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
        // The stone shows along fractures: a net of straight breaks between
        // blocks, some open wide, many closed.
        let net = gv_cells(p * 0.05 + 300.0 + (gv_fbm3(p * 0.12) - 0.5) * 0.35);
        let open_here = smoothstep(0.35, 0.75, gv_noise(p * 0.035 + 9.0));
        let crack_field = net.y + (gv_noise(p * 0.9) - 0.5) * 0.012;
        let crack_w = (0.006 + 0.022 * gv_noise(p * 0.07 + 9.0)) * open_here;
        let crack = (1.0 - smoothstep(crack_w * 0.4, crack_w, abs(crack_field))) * district * step(0.001, crack_w);
        let halo = (1.0 - smoothstep(0.0, crack_w * 5.0 + 0.001, abs(crack_field))) * district * open_here;
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

    // Water (uv.x: how wet, from the water map): soaked rock is dark and
    // glossy, and where it dries a pale mineral crust is left behind.
    let wetness = clamp(m.uv.x, 0.0, 1.0) + (gv_noise(p * 0.9 + 3.0) - 0.5) * 0.12 * step(0.02, m.uv.x);
    let moist = smoothstep(0.05, 0.3, wetness);
    let crust = smoothstep(0.4, 0.46, wetness) * (1.0 - smoothstep(0.5, 0.58, wetness)) * smoothstep(0.3, 0.6, mottle);
    let soaked = smoothstep(0.6, 0.78, wetness);
    color = color * (1.0 - 0.22 * moist);
    rough = rough - 0.2 * moist * (1.0 - threads);
    color = mix(color, vec3<f32>(0.17, 0.19, 0.165) * (0.5 + 0.7 * mottle), crust * 0.5 * (1.0 - threads));
    rough = mix(rough, 0.97, crust);
    color = color * mix(1.0, 0.45, soaked);
    rough = mix(rough, 0.1 + 0.18 * gv_noise(p * 5.0), soaked);
    n = normalize(mix(n, n0, soaked * 0.5));

    // What grows on it (the mesh colour, from flora.rs: a mat's colour
    // times how thick it lies; black is bare rock). A mat is not paint: it
    // lies in the hollows of the grain first and thins to specks at its
    // edge, and it is matt.
    let grown = base.color.rgb;
    let thick = max(grown.r, max(grown.g, grown.b));
    if thick > 0.02 {
        let hue = grown / thick;
        let nap = gv_fbm3(p * 1.3 + 11.0) * 0.55 + gv_noise(p * 6.0) * 0.3 + gv_noise(p * 21.0) * 0.15;
        let mat = smoothstep(1.02 - thick * 0.72, 1.2 - thick * 0.72, nap + 0.25 * mottle) * (1.0 - soaked);
        let pile = 0.35 + 0.9 * gv_noise(p * 17.0) * gv_noise(p * 4.3 + 7.0);
        let felt = mix(hue, vec3<f32>(dot(hue, vec3<f32>(0.3, 0.5, 0.2))), 0.3);
        color = mix(color, felt * 0.2 * pile, mat);
        rough = mix(rough, 0.93, mat);
        metal = mix(metal, 0.0, mat);
        n = normalize(mix(n, n0, mat * 0.6));
        // It gives a little light of its own, in specks.
        emission = emission * (1.0 - mat) + hue * mat * 0.3 * smoothstep(0.72, 0.9, gv_noise(p * 9.0 + m.time * 0.05));
    }

    // Rough stone has no mirror sheen at a glancing view: its grain hides
    // it. Lean the shading normal towards the eye there, so the sky's
    // reflection does not wash the strata out along a street.
    let to_eye = normalize(m.view_dir) * sign(dot(n0, m.view_dir));
    let glance = 1.0 - clamp(dot(n, to_eye), 0.0, 1.0);
    n = normalize(n + to_eye * glance * glance * 1.4 * (1.0 - metal) * (1.0 - soaked));
    o.color = vec4<f32>(color, 1.0);
    o.normal = n;
    o.roughness = clamp(rough, 0.05, 1.0);
    o.metallic = clamp(metal, 0.0, 1.0);
    o.emission = emission;
    return o;
}
"#;

/// The rock of the cavern kilometres away: the far wall, spires and
/// islands. The heart is in the middle of them all, so each is lit from
/// where the heart is as seen from it, not from one direction like the
/// near rock; the light is worked out here and given as the surface's own.
///
/// params[0].xyz: the heart's centre. params[0].w: light strength.
/// uv.y: 0 wall, 1 spire (uv.x runs root to tip), 2 island.
/// Plants: their own colour (mesh colour times the plant's tint), a waxy
/// skin, and light of their own where uv.x says so, slowly breathing,
/// each plant in its own time.
const FLORA: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    var c = base.color.rgb;
    let p = m.object_pos;
    let glow = m.uv.x;
    let kind = m.uv.y;
    let own = gv_noise(m.world_pos * 0.37);
    let breath = 0.72 + 0.28 * sin(m.time * (0.5 + own) + own * 40.0);
    var n = normalize(m.normal);
    var rough = 0.34;
    var lit = glow;
    // A waxy stem: mottled, never one flat colour.
    let mottle = gv_noise(p * 23.0) * 0.6 + gv_noise(p * 71.0) * 0.4;
    c = c * (0.7 + 0.6 * mottle);
    if kind > 1.5 {
        // A skin of cells: each its own shade, dark seams between them,
        // and the light it gives comes through the cells, not the seams.
        let cells = gv_cells(p * 26.0 + gv_noise(p * 5.0) * 0.6);
        let seam = 1.0 - smoothstep(0.0, 0.09, cells.y);
        c = c * (0.62 + 0.55 * cells.x) * (1.0 - 0.55 * seam);
        lit = glow * (0.45 + 0.9 * cells.x) * (1.0 - 0.8 * seam);
        rough = mix(0.22, 0.6, seam);
        n = normalize(n + (vec3<f32>(gv_noise(p * 60.0), gv_noise(p * 60.0 + 9.0), gv_noise(p * 60.0 + 17.0)) - 0.5) * 0.25 * (1.0 - seam));
    } else if kind > 0.5 {
        // Bark: plates with deep cracks between them, rough and matt.
        let plates = gv_cells(p * vec3<f32>(7.0, 2.6, 7.0) + gv_noise(p * 2.0) * 0.8);
        let crack = 1.0 - smoothstep(0.0, 0.12, plates.y);
        c = c * (0.55 + 0.7 * plates.x) * (1.0 - 0.7 * crack);
        rough = mix(0.7, 0.95, crack);
        n = normalize(n + (vec3<f32>(gv_noise(p * 31.0), gv_noise(p * 31.0 + 9.0), gv_noise(p * 31.0 + 17.0)) - 0.5) * 0.7);
        // Something glows faintly deep in the cracks.
        lit = glow + crack * 0.12 * plates.x;
    }
    let rim = pow(1.0 - abs(dot(n, normalize(m.view_dir))), 3.0);
    o.color = vec4<f32>(c * (1.0 - 0.5 * clamp(lit, 0.0, 1.0)), 1.0);
    o.normal = n;
    o.roughness = rough;
    o.metallic = 0.0;
    o.emission = c * (lit * 1.7 * breath + rim * 0.18);
    return o;
}
"#;

/// What people build with. uv.y says which stuff a part is made of
/// (settle.rs): 0 foamed rock, 1 lattice metal, 2 stone blocks, 3 iron,
/// 4 wood, 5 a lit pane, 6 cloth, 7 nailed boards, 8 sheet iron. uv.x is
/// the light it gives.
const BUILT: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    var c = base.color.rgb;
    let p = m.object_pos;
    let kind = m.uv.y;
    let n0 = normalize(m.normal);
    var n = n0;
    var rough = 0.8;
    var metal = 0.0;
    var emission = vec3<f32>(0.0);
    // Fine grain for the shading: two sizes that do not line up, so the
    // noise's own grid does not show as squares close up.
    let q = p.yzx * 131.0 + 4.0;
    let grit = (vec3<f32>(gv_noise(p * 59.0), gv_noise(p * 59.0 + 9.0), gv_noise(p * 59.0 + 17.0)) - 0.5) * 0.6
        + (vec3<f32>(gv_noise(q), gv_noise(q + 9.0), gv_noise(q + 17.0)) - 0.5) * 0.4;
    if kind < 0.5 {
        // Foamed rock: pale, full of small pores, cast in lifts that
        // show as faint lines, streaked dark where water runs down it.
        let pores = gv_cells(p * 15.0);
        let fine_pores = gv_cells(p * 41.0 + 3.0);
        let hole = smoothstep(0.62, 0.9, pores.x) * smoothstep(0.16, 0.05, pores.y - 0.22)
            + smoothstep(0.7, 0.95, fine_pores.x) * 0.6 * smoothstep(0.2, 0.08, fine_pores.y - 0.2);
        let lift = smoothstep(0.03, 0.0, abs(fract(p.y / 1.1 + gv_noise(p * 0.7) * 0.08) - 0.5) - 0.47);
        let streak = smoothstep(0.55, 0.8, gv_noise(vec3<f32>(p.x * 3.0, p.y * 0.25, p.z * 3.0)));
        c = c * (0.86 + 0.28 * gv_fbm3(p * 2.3)) * (1.0 - 0.34 * clamp(hole, 0.0, 1.0)) * (1.0 - 0.25 * lift) * (1.0 - 0.3 * streak);
        rough = 0.86;
        n = normalize(n + grit * 0.35 + n0 * 0.0);
    } else if kind < 1.5 {
        // Lattice metal: dark bronze gone green in patches.
        let green = smoothstep(0.45, 0.75, gv_fbm3(p * 4.0));
        c = mix(c, vec3<f32>(0.16, 0.42, 0.33), green * 0.6) * (0.75 + 0.5 * gv_noise(p * 30.0));
        metal = mix(0.95, 0.3, green);
        rough = mix(0.3, 0.75, green);
    } else if kind < 2.5 {
        // Stone blocks: heavy courses of uneven height, each block its
        // own shade, the joints cut deep; some blocks carry metal.
        let along = select(p.z, p.x, abs(n0.z) > abs(n0.x));
        let row = floor(p.y / 0.46);
        let h = gv_hash(vec3<f32>(row, 3.0, 7.0));
        let u = along / (0.7 + 0.5 * h) + h * 5.0;
        let block = vec3<f32>(floor(u), row, select(1.0, 2.0, abs(n0.z) > abs(n0.x)));
        let own = gv_hash(block);
        let ju = abs(fract(u) - 0.5);
        let jv = abs(fract(p.y / 0.46) - 0.5);
        let flat_top = smoothstep(0.6, 0.9, abs(n0.y));
        let joint = max(smoothstep(0.45, 0.49, ju) * (0.46 * (0.7 + 0.5 * h) / 0.46), smoothstep(0.44, 0.49, jv)) * (1.0 - flat_top);
        let vein = smoothstep(0.78, 0.92, gv_noise(p * vec3<f32>(5.0, 14.0, 5.0) + own * 20.0)) * step(0.7, own);
        c = c * (0.5 + 0.8 * own) * (0.8 + 0.4 * gv_fbm3(p * 5.0)) * (1.0 - 0.72 * clamp(joint, 0.0, 1.0));
        c = mix(c, vec3<f32>(0.5, 0.42, 0.3), vein * 0.7);
        metal = vein * 0.8;
        rough = mix(0.88, 0.4, vein);
        n = normalize(n + grit * 0.5 * (1.0 - joint));
    } else if kind < 3.5 {
        // Iron: dark, hammered, rusting from the edges.
        let rust = smoothstep(0.5, 0.8, gv_fbm3(p * 3.0 + 5.0));
        c = mix(c * (0.8 + 0.4 * gv_noise(p * 24.0)), vec3<f32>(0.3, 0.12, 0.05) * (0.6 + 0.8 * gv_noise(p * 40.0)), rust * 0.7);
        metal = mix(0.9, 0.1, rust);
        rough = mix(0.45, 0.95, rust);
        n = normalize(n + grit * 0.3);
    } else if kind < 4.5 {
        // Wood: grain along its length, grey with age.
        let long = select(p.x, p.y, abs(n0.y) < 0.6);
        let grain = gv_noise(vec3<f32>(p.x * 31.0, long * 1.3, p.z * 31.0));
        let knots = smoothstep(0.8, 0.95, gv_noise(p * vec3<f32>(6.0, 2.0, 6.0)));
        c = c * (0.6 + 0.7 * grain) * (1.0 - 0.6 * knots);
        c = mix(c, vec3<f32>(dot(c, vec3<f32>(0.33))), 0.35 * gv_fbm3(p * 1.5));
        rough = 0.9;
        n = normalize(n + grit * 0.4);
    } else if kind < 5.5 {
        // A lit pane: leaded into small quarrels, each a little its own
        // brightness, the light behind it never quite still.
        let quarrel = gv_cells(p * vec3<f32>(9.0, 6.0, 9.0));
        let lead = 1.0 - smoothstep(0.0, 0.07, quarrel.y);
        let flick = 0.82 + 0.18 * sin(m.time * (1.0 + quarrel.x * 0.1) * 2.3 + gv_noise(m.world_pos * 0.5) * 40.0);
        let lit = m.uv.x * (0.55 + 0.6 * quarrel.x) * (1.0 - lead) * flick;
        c = c * (0.25 + 0.2 * quarrel.x) * (1.0 - 0.8 * lead);
        emission = base.color.rgb * base.color.rgb * lit * 1.35;
        rough = 0.18;
    } else if kind < 6.5 {
        // Cloth: a coarse weave, faded and stained, hanging in folds.
        let weave = 0.9 + 0.1 * sin(p.x * 260.0) * sin(p.y * 260.0);
        let grime = gv_fbm3(p * 1.1 + 4.0);
        c = mix(c, vec3<f32>(dot(c, vec3<f32>(0.33))), 0.45) * weave * (0.35 + 0.75 * grime);
        let side = abs(n0.z) > abs(n0.x);
        let a = select(p.z, p.x, side);
        let hang = a * 11.0 + gv_noise(p * 1.7) * 5.0 + p.y * 1.3;
        let fold = cos(hang) * (0.35 + 0.65 * gv_noise(p * 0.9 + 8.0));
        c = c * (0.8 + 0.2 * sin(hang));
        n = normalize(n + select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), side) * fold * 0.5);
        rough = 1.0;
    } else if kind < 7.5 {
        // Boards nailed side by side: each its own shade and grain, gaps
        // between them, knots, a nail near each end, grey with age.
        let lying = abs(n0.y) > 0.7;
        let side = abs(n0.z) > abs(n0.x) || lying;
        let a = select(p.z, p.x, side);
        let l = select(p.y, p.z, lying);
        let u = a / 0.16;
        let bh = gv_hash(vec3<f32>(floor(u), 3.0, 11.0));
        let edge = abs(fract(u) - 0.5);
        let gap = smoothstep(0.44, 0.495, edge);
        let wob = gv_noise(vec3<f32>(floor(u) * 7.0, l * 0.9, 2.0));
        let rings = gv_noise(vec3<f32>((fract(u) + wob * 0.6) * 9.0 + bh * 30.0, l * 0.7 + bh * 13.0, bh * 5.0));
        let fibre = gv_noise(vec3<f32>(a * 140.0, l * 4.0, bh * 9.0));
        let knots = smoothstep(0.86, 0.97, gv_noise(vec3<f32>(a * 9.0 + bh * 5.0, l * 2.2, 3.0)));
        c = c * (0.55 + 0.7 * bh) * (0.7 + 0.45 * rings) * (0.85 + 0.3 * fibre) * (1.0 - 0.65 * knots);
        c = mix(c, vec3<f32>(dot(c, vec3<f32>(0.33))) * 1.1, 0.2 + 0.4 * gv_fbm3(p * 1.5));
        c = c * (1.0 - 0.88 * gap);
        let nail_at = vec2<f32>((fract(u) - 0.5) * 0.16, (fract(l / 0.85 + bh) - 0.5) * 0.85);
        let nail = 1.0 - smoothstep(0.007, 0.012, length(nail_at));
        let weep = (1.0 - smoothstep(0.0, 0.012, abs(nail_at.x))) * smoothstep(0.0, -0.12, nail_at.y) * step(nail_at.y, 0.0)
            * (1.0 - step(0.7, abs(n0.y)));
        c = mix(c, vec3<f32>(0.2, 0.09, 0.04), weep * 0.5);
        c = mix(c, vec3<f32>(0.07, 0.06, 0.06), nail);
        metal = nail * 0.8;
        rough = mix(0.92, 0.5, nail);
        let across = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 0.0), side);
        n = normalize(n + across * (fract(u) - 0.5) * 0.9 * smoothstep(0.3, 0.5, edge) + grit * 0.3);
    } else {
        // Sheet iron: beaten flat by hand, so it is dented all over; rust
        // comes in patches and runs down it, pitted where it is deep.
        let dent = vec3<f32>(gv_noise(p * 6.0 + 1.0), gv_noise(p * 6.0 + 5.0), gv_noise(p * 6.0 + 9.0)) - 0.5;
        let run = gv_noise(vec3<f32>(p.x * 9.0, p.y * 0.6, p.z * 9.0));
        let rust = smoothstep(0.42, 0.72, gv_fbm3(p * 2.2 + 5.0) * 0.65 + run * 0.45);
        let pit = smoothstep(0.55, 0.9, gv_noise(p * 71.0 + 2.0));
        let steel = vec3<f32>(0.2, 0.2, 0.22) * (0.6 + 0.6 * gv_noise(p * 23.0)) * (0.7 + 0.6 * gv_fbm3(p * 1.3));
        let rusty = mix(vec3<f32>(0.26, 0.1, 0.04), vec3<f32>(0.4, 0.19, 0.07), gv_noise(p * 31.0 + 6.0)) * (1.0 - 0.5 * pit);
        c = mix(steel, mix(rusty, c, 0.35), rust * 0.9);
        metal = mix(0.9, 0.05, rust);
        rough = mix(0.36, 0.96, rust);
        n = normalize(n + dent * 0.4 + grit * 0.35 * rust);
    }
    o.color = vec4<f32>(c, 1.0);
    o.normal = n;
    o.roughness = rough;
    o.metallic = metal;
    o.emission = emission;
    return o;
}
"#;

const CAVERN: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let p = m.world_pos;
    let n = normalize(m.normal);
    let to_heart = m.params[0].xyz - p;
    let dist = length(to_heart);
    let l = to_heart / dist;

    // Folded strata in broad bands, as on the near rock (L7, L9).
    let fold = gv_fbm3(p * 0.0013 + 7.0);
    // Broken into great blocks that have slipped against each other.
    let blocks = gv_cells_far(p * vec3<f32>(0.0031, 0.0022, 0.0027) + (fold - 0.5) * 0.4);
    let joint = 1.0 - smoothstep(0.0, 0.03, blocks.y);
    let layer = dot(p, vec3<f32>(0.31, 0.52, 0.80)) * 0.011 + fold * 2.2 + (blocks.x - 0.5) * 2.4
        + (gv_noise(p * 0.03) - 0.5) * 0.12;
    let band = gv_noise(vec3<f32>(layer * 2.3, 1.7, 4.2));
    let fine = gv_noise(vec3<f32>(layer * 11.0, 3.1, 0.4));
    let mottle = gv_fbm3(p * 0.006 + 19.0);
    var tone = 0.06 + 0.13 * band * band + 0.04 * fine + 0.09 * mottle + 0.05 * blocks.x;
    tone = tone * (1.0 - 0.5 * joint);
    // Bays lie in their own shade, buttresses stand in the light.
    let wall = step(m.uv.y, 0.5);
    tone = tone * mix(1.0, 0.5 + 0.75 * m.uv.x, wall);
    var color = vec3<f32>(0.62, 0.66, 0.78) * tone;
    let ochre = smoothstep(0.62, 0.8, gv_noise(vec3<f32>(layer * 0.9, 8.0, 2.0)));
    color = mix(color, vec3<f32>(0.75, 0.62, 0.42) * tone, ochre * 0.55);

    // What grows on it, seen from far off: mats in territories with bare
    // lines between them (flora.rs draws the near ones one by one):
    // crimson in the heart's light, violet in the shade, teal here and
    // there where it is damp; none on the joints and the barest faces.
    let ground = gv_cells_far(p * 0.0075 + 500.0 + (fold - 0.5) * 0.6);
    let patchy = smoothstep(0.4, 0.6, gv_fbm3(p * 0.0024 + 90.0)) * (0.35 + 0.65 * ground.x);
    let lit_side = smoothstep(0.1, 0.5, dot(n, l));
    var hue = mix(vec3<f32>(0.34, 0.1, 0.6), vec3<f32>(0.62, 0.05, 0.12), lit_side);
    hue = mix(hue, vec3<f32>(0.05, 0.5, 0.42), step(0.74, ground.x) * (1.0 - 0.6 * lit_side));
    let grown = patchy * smoothstep(0.0, 0.05, ground.y) * (1.0 - joint);
    color = mix(color, hue * (0.1 + 0.12 * mottle), grown * 0.8);

    // The heart's light: square on, the rock is bleached (L10); turned
    // away it keeps a little of the glow the mist carries everywhere.
    let facing = dot(n, l);
    let lit = smoothstep(-0.1, 1.0, facing);
    let reach = clamp(pow(6000.0 / dist, 1.3), 0.5, 3.5);
    let heart_light = vec3<f32>(0.72, 0.84, 1.0) * lit * lit * reach * m.params[0].w;
    let mist_light = vec3<f32>(0.10, 0.15, 0.26) * 0.2 * m.params[0].w;
    var emission = color * (heart_light + mist_light);

    // Lodestone veins showing through as long amber seams (L2).
    let district = smoothstep(0.5, 0.72, gv_fbm3(p * 0.0009 + 200.0));
    let net = gv_cells_far(p * 0.0042 + 300.0 + (fold - 0.5) * 0.3);
    let seam = (1.0 - smoothstep(0.0, 0.03, net.y)) * smoothstep(0.4, 0.7, gv_noise(p * 0.0028 + 9.0));
    let pulse = 0.75 + 0.25 * sin(m.time * 0.4 + gv_noise(p * 0.002) * 12.0);
    emission = emission + vec3<f32>(1.0, 0.36, 0.04) * seam * district * 0.9 * pulse;

    // Where people live: lamps, in towns that cling to spires and wall.
    let spire = step(0.5, m.uv.y) * step(m.uv.y, 1.5);
    let town = smoothstep(0.6 - 0.14 * spire, 0.74 - 0.14 * spire, gv_fbm3(p * 0.0017 + 50.0));
    let cell = floor(p / 34.0);
    let lamp = step(0.8, gv_hash(cell));
    let in_cell = length(fract(p / 34.0) - vec3<f32>(0.5));
    let glow = lamp * town * (1.0 - smoothstep(0.0, 0.17, in_cell));
    emission = emission + vec3<f32>(1.0, 0.62, 0.28) * glow * 4.0;

    o.color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    o.roughness = 1.0;
    o.metallic = 0.0;
    o.emission = emission;
    return o;
}
"#;

/// Water standing and running on the rock: dark, clear at the edge, with
/// a slow ripple. uv.x: depth in metres.
const WATER: &str = r#"
fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let p = m.world_pos;
    let n0 = normalize(m.normal);
    let t = m.time * 0.35;
    let g = vec3<f32>(
        gv_noise(p * 1.9 + vec3<f32>(t, 0.0, 0.0)),
        gv_noise(p * 1.9 + vec3<f32>(17.0, t, 0.0)),
        gv_noise(p * 1.9 + vec3<f32>(0.0, 31.0, t))) - vec3<f32>(0.5);
    let deep = smoothstep(0.0, 1.6, m.uv.x);
    let ripple = (g - n0 * dot(g, n0)) * mix(0.22, 0.1, deep);
    o.normal = normalize(n0 + ripple);
    let shallow = vec3<f32>(0.012, 0.03, 0.034);
    let depth_color = vec3<f32>(0.004, 0.02, 0.03);
    o.color = vec4<f32>(mix(shallow, depth_color, deep), mix(0.45, 0.94, deep));
    o.roughness = mix(0.22, 0.03, smoothstep(0.0, 0.3, m.uv.x));
    o.metallic = 0.0;
    o.emission = vec3<f32>(0.0);
    return o;
}
"#;

/// How strong the heart's light is on the far rock, matched by eye to the
/// engine's own light on the near rock.
const CAVERN_LIGHT: f32 = 0.5;

/// The materials and the heart's body, made once.
pub struct Look {
    spire: CustomMaterialId,
    wall: CustomMaterialId,
    island: CustomMaterialId,
    far: CustomMaterialId,
    cavern: CustomMaterialId,
    water: CustomMaterialId,
    /// What plants are made of.
    pub flora: CustomMaterialId,
    /// What buildings are made of.
    pub built: CustomMaterialId,
    /// Draws nothing: for shapes that are only there to be solid.
    pub unseen: CustomMaterialId,
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
        // The heart stands where it is, at its true size.
        world.set_place(
            blades,
            Place::new(heart_center.x, heart_center.y, heart_center.z).with_scale(HEART_CORE),
        )?;
        world.set_casts_shadow(blades, false)?;
        let cavern = world.add_material(
            CustomMaterial::new("cavern rock", format!("{NOISE}{CAVERN}")).param(
                0,
                [heart_center.x, heart_center.y, heart_center.z, CAVERN_LIGHT],
            ),
        )?;
        let water = world.add_material(
            CustomMaterial::new("water on rock", format!("{NOISE}{WATER}")).see_through(),
        )?;
        let flora = world.add_material(CustomMaterial::new("flora", format!("{NOISE}{FLORA}")))?;
        let built = world.add_material(CustomMaterial::new("built", format!("{NOISE}{BUILT}")))?;
        let unseen = world.add_material(
            CustomMaterial::new(
                "unseen",
                "fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {\n    var o = base;\n    o.color = vec4<f32>(0.0);\n    return o;\n}\n",
            )
            .see_through(),
        )?;
        Ok(Self {
            flora,
            built,
            unseen,
            spire,
            wall,
            island,
            far,
            cavern,
            water,
            heart_dir,
        })
    }

    pub fn cavern_material(&self) -> CustomMaterialId {
        self.cavern
    }

    pub fn rock_material(&self, kind: RockKind) -> CustomMaterialId {
        match kind {
            RockKind::Spire => self.spire,
            RockKind::Wall => self.wall,
            RockKind::Island => self.island,
            RockKind::FarSpire => self.far,
            RockKind::Water => self.water,
        }
    }

    /// Light and air: the heart as the one great light, deep cool shadow,
    /// mist that takes the heart's colour (Q5).
    pub fn light_and_air(&self, world: &mut World) -> EngineResult<()> {
        // The mist carries the heart's light through the whole hollow: far
        // rock pales into a blue glow, it never goes black.
        let mist = rgb(38, 52, 80);
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
        let mut haze = Haze::new(mist, 20_000.0);
        haze.base_y = -100_000.0;
        haze.height_m = 1_000_000.0;
        world.set_haze(Some(haze));
        world.set_view_distance(16_000.0)?;
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
}
