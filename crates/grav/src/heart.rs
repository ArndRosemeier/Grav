//! The heart's body: not a ball but a splintered mass of repelling
//! lodestone, a ragged knot of glassy blades and spines with light breaking
//! out between them (design doc H3, H4; style L5: repelling stone is glassy
//! and splintered). A few sprays of giant spines and a lopsided core keep
//! its outline from ever reading as round, and a loose halo of shed shards
//! drifts around it.
//!
//! The mesh is in units of the core's radius. Each point's texture
//! coordinate carries data for the material: x is the blade's own random
//! number (its clock and pattern), y is how far along the blade the point
//! lies (0 root, 1 tip).

use engine::prelude::*;
use std::f32::consts::TAU;

/// A small deterministic random source (same heart every run).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32) / ((1u64 << 24) as f32)
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }

    /// A direction spread evenly over the sphere.
    fn direction(&mut self) -> Vec3 {
        let y = self.range(-1.0, 1.0);
        let a = self.range(0.0, TAU);
        let r = (1.0 - y * y).max(0.0).sqrt();
        Vec3::new(r * a.cos(), y, r * a.sin())
    }
}

/// The data of a point, carried in its texture coordinate: x is the
/// blade's own random number, y is how far along the blade it lies.
fn data(blade: f32, along: f32) -> [f32; 2] {
    [blade, along]
}

/// One triangle, turned so its front faces away from `inside`.
fn tri(mesh: &mut Mesh, p: [Vec3; 3], c: [[f32; 2]; 3], inside: Vec3) -> EngineResult<()> {
    let n = (p[1] - p[0]).cross(p[2] - p[0]);
    let order = if n.dot(p[0] - inside) >= 0.0 {
        [0, 1, 2]
    } else {
        [0, 2, 1]
    };
    let mut ids = [None; 3];
    for (slot, &k) in ids.iter_mut().zip(&order) {
        let id = mesh.add_point(p[k])?;
        mesh.set_point_uv(id, c[k])?;
        *slot = Some(id);
    }
    if let [Some(a), Some(b), Some(cc)] = ids {
        mesh.add_triangle(a, b, cc)?;
    }
    Ok(())
}

/// A blade of crystal: a faceted prism from `root` along `axis`, widest a
/// little out from the root, running to a point at `length`.
fn blade(
    mesh: &mut Mesh,
    rng: &mut Rng,
    root: Vec3,
    axis: Vec3,
    length: f32,
    width: f32,
    along: (f32, f32),
) -> EngineResult<()> {
    let tone = |t: f32| along.0 + (along.1 - along.0) * t;
    let sides = 5 + (rng.next() * 3.0) as usize;
    let side = axis.any_orthonormal_vector();
    let up = axis.cross(side);
    let id = rng.next();
    // A slight bend, so no two spines are parallel rulers.
    let bend = (side * rng.range(-1.0, 1.0) + up * rng.range(-1.0, 1.0)) * length * 0.06;
    // Uneven sides: a crystal, not a turned part.
    let radii: Vec<f32> = (0..sides).map(|_| rng.range(0.6, 1.25)).collect();
    let twist = rng.range(0.0, TAU);
    let stations = [(0.0_f32, 0.55_f32), (0.16, 1.0), (0.62, 0.6)];
    let ring = |t: f32, scale: f32| -> Vec<Vec3> {
        let centre = root + axis * (length * t) + bend * (t * t);
        (0..sides)
            .map(|i| {
                let a = twist + i as f32 / sides as f32 * TAU;
                centre + (side * a.cos() + up * a.sin()) * (width * scale * radii[i])
            })
            .collect()
    };
    let rings: Vec<(f32, Vec<Vec3>)> = stations.iter().map(|&(t, s)| (t, ring(t, s))).collect();
    for pair in rings.windows(2) {
        let ((t0, r0), (t1, r1)) = (&pair[0], &pair[1]);
        let inside = root + axis * (length * (t0 + t1) * 0.5) + bend * (t0 * t0);
        for i in 0..sides {
            let j = (i + 1) % sides;
            let (c0, c1) = (data(id, tone(*t0)), data(id, tone(*t1)));
            tri(mesh, [r0[i], r0[j], r1[j]], [c0, c0, c1], inside)?;
            tri(mesh, [r0[i], r1[j], r1[i]], [c0, c1, c1], inside)?;
        }
    }
    let (t_last, last) = &rings[rings.len() - 1];
    let tip = root + axis * length + bend;
    let inside = root + axis * (length * 0.8) + bend * 0.64;
    for i in 0..sides {
        let j = (i + 1) % sides;
        let c = data(id, tone(*t_last));
        tri(
            mesh,
            [last[i], last[j], tip],
            [c, c, data(id, tone(1.0))],
            inside,
        )?;
    }
    Ok(())
}

/// Smaller crystals growing out of the side of a big blade, leaning along
/// it, the way real crystals crowd each other.
fn side_growth(
    mesh: &mut Mesh,
    rng: &mut Rng,
    root: Vec3,
    axis: Vec3,
    length: f32,
    width: f32,
) -> EngineResult<()> {
    if length < 0.7 {
        return Ok(());
    }
    let count = 2 + (rng.next() * 4.0) as usize;
    for _ in 0..count {
        let t = rng.range(0.08, 0.55);
        let out = (rng.direction().cross(axis)).normalize_or(axis.any_orthonormal_vector());
        let from = root + axis * (length * t) + out * (width * 0.3);
        let lean = (axis * rng.range(0.6, 1.3) + out).normalize();
        let child = length * rng.range(0.12, 0.34);
        let child_width = child * rng.range(0.1, 0.2) + 0.01;
        blade(mesh, rng, from, lean, child, child_width, (0.1, 1.0))?;
    }
    Ok(())
}

/// The heart's body in units of its core radius, centred on the origin.
pub fn body() -> EngineResult<Mesh> {
    let mut mesh = Mesh::new();
    let mut rng = Rng(0x6ea7_c0de);
    // A lopsided core: longer one way, pinched another.
    let squash = Vec3::new(1.45, 0.8, 1.05);
    let tilt = Quat::from_euler(engine::glam::EulerRot::YXZ, 0.7, 0.4, 0.3);
    let on_core = |dir: Vec3, depth: f32| tilt * (dir * squash * depth);
    // The mass of the body grows in clusters, like a druse: blades of one
    // cluster lean the same way, and between clusters the core shows.
    for _ in 0..17 {
        let towards = rng.direction();
        let size = rng.range(0.45, 1.5);
        let count = 14 + (rng.next() * 16.0) as usize;
        for _ in 0..count {
            let axis = (tilt * (towards * squash) + rng.direction() * 0.3).normalize();
            let from = (towards + rng.direction() * 0.45).normalize();
            let root = on_core(from, rng.range(0.3, 0.75));
            let u = rng.next();
            let length = size * (0.35 + 1.25 * u * u);
            let width = length * rng.range(0.08, 0.17) + 0.02;
            blade(&mut mesh, &mut rng, root, axis, length, width, (0.0, 1.0))?;
            side_growth(&mut mesh, &mut rng, root, axis, length, width)?;
        }
    }
    // The core is crystal too: stubby splinters packed so close that no
    // gap shows through, down to the middle.
    for _ in 0..520 {
        let dir = rng.direction();
        let root = on_core(dir, rng.range(0.1, 0.6));
        let axis = (tilt * (dir * squash) + rng.direction() * 0.8).normalize();
        let length = rng.range(0.25, 0.65);
        let width = length * rng.range(0.22, 0.42);
        blade(&mut mesh, &mut rng, root, axis, length, width, (0.25, 1.0))?;
    }
    // Three sprays of giant spines, so the outline is never a disc.
    let sprays = [
        (Vec3::new(-0.9, 0.35, 0.25), 9, 3.2),
        (Vec3::new(0.55, -0.75, 0.35), 7, 2.5),
        (Vec3::new(0.3, 0.6, -0.75), 6, 2.1),
    ];
    for (towards, count, reach) in sprays {
        let towards = towards.normalize();
        for _ in 0..count {
            let axis = (towards + rng.direction() * 0.33).normalize();
            let root = on_core(axis, rng.range(0.2, 0.5));
            let length = reach * rng.range(0.55, 1.0);
            let width = length * rng.range(0.035, 0.06);
            blade(&mut mesh, &mut rng, root, axis, length, width, (0.0, 1.0))?;
            side_growth(&mut mesh, &mut rng, root, axis, length, width)?;
        }
    }
    // Shards shed by flares: a loose, uneven halo, thicker towards the sprays.
    for _ in 0..110 {
        let mut dir = rng.direction();
        if rng.next() < 0.6 {
            dir = (dir + sprays[(rng.next() * 3.0) as usize % 3].0.normalize() * 1.2).normalize();
        }
        let at = dir * rng.range(1.5, 3.3);
        let axis = rng.direction();
        let length = rng.range(0.04, 0.2);
        let width = length * rng.range(0.1, 0.22);
        // A shard is a blade broken off: pointed at both ends.
        blade(&mut mesh, &mut rng, at, axis, length, width, (0.35, 0.6))?;
        let back = length * rng.range(0.3, 0.8);
        blade(&mut mesh, &mut rng, at, -axis, back, width, (0.35, 0.6))?;
    }
    Ok(mesh)
}

/// The blades' material: dark glass that carries its own light. The glow is
/// not a lamp at the centre: every blade has its own level, clock and
/// pattern (clouds of light, veins, bands that travel along it, a burning
/// tip on some), so bright and dark blades sit side by side all through
/// the mass. The light is looked up at several depths behind the surface,
/// sharp near it and soft and bluer deep down, so it sits inside the glass;
/// the surface itself only adds what glass adds: lit edges and facets that
/// flash now and then.
///
/// params[0].x: brightness. params[0].y: the entity's scale. Needs the
/// shared noise functions before it.
pub const BLADES: &str = r#"
// The light at one place inside a blade: x clouds with their field lines,
// y veins. `soft` widens everything for the deep layers.
fn heart_light(p: vec3<f32>, blade: f32, h2: f32, h3: f32, t: f32, soft: f32) -> vec2<f32> {
    let q = p * (1.6 + 1.8 * h3) + vec3<f32>(blade * 40.0, t * 0.03, -t * 0.02);
    let field = gv_fbm3(q);
    let clouds = smoothstep(0.53 - 0.05 * soft, 0.74, field);
    let lines = pow(0.5 + 0.5 * sin(field * (64.0 - 30.0 * soft) + t * 0.3), 5.0 - 3.0 * soft);
    let vf = gv_fbm3(p * (3.0 + 4.0 * h2) + blade * 23.0 + vec3<f32>(0.0, t * 0.015, 0.0));
    let veins = 1.0 - smoothstep(0.0, 0.010 + 0.03 * soft, abs(vf - 0.5));
    return vec2<f32>(clouds * (0.3 + 0.7 * lines), veins);
}

fn material(m: MaterialIn, base: MaterialOut) -> MaterialOut {
    var o = base;
    let blade = m.uv.x;
    let along = m.uv.y;
    let t = m.time;
    // The engine gives the object position with the entity's scale applied:
    // take it back to the mesh's own units (core radii).
    let p = m.object_pos / max(m.params[0].y, 0.001);
    let n = normalize(m.normal);
    let v0 = normalize(m.view_dir);
    let v = v0 * sign(dot(n, v0) + 0.0001);
    let h1 = gv_hash(vec3<f32>(blade * 91.0, 3.0, 7.0));
    let h2 = gv_hash(vec3<f32>(blade * 57.0, 11.0, 2.0));
    let h3 = gv_hash(vec3<f32>(blade * 33.0, 5.0, 19.0));
    // How alive this blade is: many are nearly dark, a few blaze. Blades in
    // one region share a slow swell, and each has its own faster clock.
    let region = floor(normalize(p) * 2.3 + 0.5);
    let swell = 0.5 + 0.5 * sin(t * (0.15 + 0.25 * gv_hash(region)) + gv_hash(region + 5.0) * 40.0);
    let own = 0.5 + 0.5 * sin(t * (0.35 + 0.8 * h2) + h1 * 50.0);
    let level = pow(h1, 3.4) * 1.5 * (0.35 + 0.65 * swell) * (0.5 + 0.5 * own) + 0.02;

    // Look into the glass: the sight line bends towards the facet's normal.
    let into = normalize(-v * 0.75 - n * 0.45);
    let near = heart_light(p + into * 0.012, blade, h2, h3, t, 0.0);
    let middle = heart_light(p + into * 0.05, blade, h2, h3, t, 0.5);
    let far = heart_light(p + into * 0.12, blade, h2, h3, t, 1.0);

    // Bands that travel along the blade, outward on some, inward on others.
    let way = select(-1.0, 1.0, h3 > 0.4);
    let band_phase = along * (6.0 + 16.0 * h2) - way * t * (0.25 + 0.5 * h1) + far.x * 3.0;
    let bands = pow(0.5 + 0.5 * sin(band_phase), 12.0) * (0.2 + 0.8 * middle.x);
    // Some blades burn at the tip; the rest run out dark against the glow.
    let burns = step(0.8, h3);
    let tip = smoothstep(0.7, 1.0, along);
    let body = 1.0 - tip * (1.0 - burns) * 0.85;

    let deep = vec3<f32>(0.003, 0.010, 0.060);
    let blue = vec3<f32>(0.03, 0.16, 0.85);
    let mid = vec3<f32>(0.09, 0.33, 1.00);
    let hot = vec3<f32>(0.74, 0.90, 1.00);
    // The glass: nearly black blue, each facet its own depth.
    let facet = gv_hash(floor(n * 5.0 + 0.5) + blade * 3.0);
    var e = deep * (0.3 + 1.0 * facet);
    // Deep light: soft and blue, it fills the blade.
    e += blue * (far.x * 1.6 + far.y * 0.8) * level * 1.5;
    // Middle depth.
    e += mid * (middle.x * 1.3 + middle.y * 1.6) * level * 2.4;
    // Just under the surface: sharp and white hot.
    e += mix(mid, hot, 0.75) * (near.x * near.x * 1.8 + near.y * 3.2) * level * 2.6;
    e += mix(mid, hot, bands) * bands * level * 5.0;
    e += hot * tip * burns * level * 7.0;
    e = e * body;
    // What the surface adds. Edges glow with the light inside the blade.
    let rim = pow(1.0 - abs(dot(n, v)), 3.0);
    e += mid * rim * (0.02 + 0.7 * level) * body;
    // Facets flash as they catch the light of their neighbours: three slow
    // wandering lights, a whole facet at a time.
    let r = reflect(-v, n);
    let a1 = t * 0.05;
    let l1 = normalize(vec3<f32>(cos(a1), 0.35, sin(a1)));
    let l2 = normalize(vec3<f32>(-sin(a1 * 0.7), cos(a1 * 0.7), 0.4));
    let l3 = normalize(vec3<f32>(0.3, -cos(a1 * 1.3), sin(a1 * 1.3)));
    let flash = pow(max(dot(r, l1), 0.0), 220.0) + pow(max(dot(r, l2), 0.0), 220.0)
        + pow(max(dot(r, l3), 0.0), 220.0);
    e += hot * flash * 1.4;
    o.color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    o.roughness = 0.2;
    o.metallic = 0.0;
    o.emission = e * m.params[0].x;
    return o;
}
"#;
