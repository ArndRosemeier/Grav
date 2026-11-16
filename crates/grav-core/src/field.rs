//! The total attraction field: veins, the heart and all open stones (A6).

use crate::source::{Charge, Source, SourceId};
use crate::vein::Vein;
use crate::G;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// The heart's steady push (H6, H7): outward from its centre, falling
/// linearly from the edge of the stripped zone to the outer wall.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Heart {
    pub center: Vec3,
    /// Radius of the stripped zone, metres (about 3 km).
    pub stripped_radius: f32,
    /// Distance of the outer wall from the centre, metres.
    pub wall_radius: f32,
    /// Push at the stripped zone's edge, in g (0.3).
    pub push_at_stripped: f32,
    /// Push at the outer wall, in g (0.02).
    pub push_at_wall: f32,
    /// Heart weather's push value for the region (1 = normal).
    pub push_scale: f32,
    /// A flare's extra outward push in g (0 when there is none). Vein
    /// shielding does not stop it; inside an inhabited vein's hold zone it
    /// is capped at half the vein's pull there (H11).
    #[serde(default)]
    pub flare_g: f32,
}

impl Heart {
    /// Push in g at `r` metres from the centre.
    pub fn push_g(&self, r: f32) -> f32 {
        let span = (self.wall_radius - self.stripped_radius).max(1.0);
        let t = ((r - self.stripped_radius) / span).clamp(0.0, 1.0);
        (self.push_at_stripped + (self.push_at_wall - self.push_at_stripped) * t) * self.push_scale
    }

    /// Unshielded push at `p` in m/s².
    pub fn push(&self, p: Vec3) -> Vec3 {
        let d = p - self.center;
        let r = d.length();
        if r < 1e-3 {
            return Vec3::ZERO;
        }
        d / r * self.push_g(r) * G
    }

    /// The flare's push at `p` in m/s², before the cap of H11.
    pub fn flare(&self, p: Vec3) -> Vec3 {
        let d = p - self.center;
        let r = d.length();
        if r < 1e-3 || self.flare_g <= 0.0 {
            return Vec3::ZERO;
        }
        d / r * self.flare_g * G
    }
}

/// A flare's push where veins pull with `veins` (m/s²) and an inhabited
/// vein holds the point by `inhabited_hold` (0..=1): it breaks through
/// shielding, but in a town it is capped at half the local pull (H11).
fn capped_flare(flare: Vec3, veins: Vec3, inhabited_hold: f32) -> Vec3 {
    let free = flare.length();
    let cap = free + (0.5 * veins.length() - free).min(0.0) * inhabited_hold;
    flare.clamp_length_max(cap.max(0.0))
}

/// A body that is itself lodestone (a stone lying loose, held or thrown).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LodestoneBody {
    pub charge: Charge,
    /// Sheath opening `0..=1`; 0 means fully coated (inert, A11).
    pub opening: f32,
    /// The stone's own source, which never acts on itself.
    pub source: Option<SourceId>,
}

/// What the field is asked about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyQuery {
    /// The game's id for the body (a held stone skips its holder, P8).
    pub id: u64,
    pub position: Vec3,
    pub mass: f32,
    pub lodestone: Option<LodestoneBody>,
}

impl BodyQuery {
    /// Ordinary matter at `position`.
    pub fn at(position: Vec3) -> Self {
        Self {
            id: u64::MAX,
            position,
            mass: 1.0,
            lodestone: None,
        }
    }
}

/// The field's parts at one point, for drawing and for the hero's sense.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FieldParts {
    pub veins: Vec3,
    /// The heart's push after vein shielding.
    pub heart: Vec3,
    pub stones: Vec3,
    /// How strongly a vein holds this point, `0..=1` (1 = full hold zone).
    pub vein_hold: f32,
}

impl FieldParts {
    pub fn total(&self) -> Vec3 {
        self.veins + self.heart + self.stones
    }
}

/// A snapshot of every layer of the field. Cheap to clone; the game
/// rebuilds the stone list when stones change and hands the snapshot to
/// physics.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Field {
    pub veins: Vec<Vein>,
    pub heart: Option<Heart>,
    pub sources: Vec<Source>,
}

impl Field {
    /// Up at `at`: against the pull there (for a thing standing square to
    /// it), or `or` where nothing pulls.
    pub fn up_at(&self, at: Vec3, or: Vec3) -> Vec3 {
        (-self.acceleration(&BodyQuery::at(at))).normalize_or(or)
    }

    /// The acceleration acting on a body (A1 to A15), in m/s².
    pub fn acceleration(&self, q: &BodyQuery) -> Vec3 {
        let ordinary = self.parts(q).total();
        let Some(stone) = q.lodestone else {
            return ordinary;
        };
        // A14: a stone open by u answers ordinary fields with 1 - u and
        // opposite-charge repulsion with u. Fully coated: plain matter (A11).
        let u = stone.opening.clamp(0.0, 1.0);
        if u <= 0.0 {
            return ordinary;
        }
        ordinary * (1.0 - u) + self.repulsion(q, stone) * u
    }

    /// The summed potential of all veins at `p`, in g times metres
    /// (`Vein::potential`). Rock that has settled under the veins' pull
    /// lies on a surface where this is equal everywhere.
    pub fn vein_potential(&self, p: Vec3) -> f32 {
        self.veins.iter().map(|v| v.potential(p)).sum()
    }

    /// The parts of the ordinary field at a body (no charge rules).
    pub fn parts(&self, q: &BodyQuery) -> FieldParts {
        let p = q.position;
        let mut parts = FieldParts::default();
        let mut inhabited: f32 = 0.0;
        for v in &self.veins {
            if !v.may_reach(p) {
                continue;
            }
            let (a, share) = v.pull(p);
            parts.veins += a;
            parts.vein_hold = parts.vein_hold.max(share);
            if v.inhabited {
                inhabited = inhabited.max(share);
            }
        }
        if let Some(h) = &self.heart {
            // A7: inside a vein's hold zone the steady push is shielded,
            // fading back in across the fade band.
            parts.heart = h.push(p) * (1.0 - parts.vein_hold)
                + capped_flare(h.flare(p), parts.veins, inhabited);
        }
        let own = q.lodestone.and_then(|l| l.source);
        for s in &self.sources {
            if s.holder == Some(q.id) || Some(s.id) == own || !s.may_reach(p) {
                continue;
            }
            parts.stones += s.acceleration(p, q.mass);
        }
        parts
    }

    /// A13: push straight away from every source of the opposite charge
    /// the stone is inside, as strong as that source's field there.
    fn repulsion(&self, q: &BodyQuery, stone: LodestoneBody) -> Vec3 {
        let p = q.position;
        let mut out = Vec3::ZERO;
        let opposite = stone.charge.opposite();
        if opposite == Charge::Attracting {
            for v in &self.veins {
                let (a, _) = v.pull(p);
                // The vein pulls towards its surface; away is the reverse.
                out -= a;
            }
        }
        if opposite == Charge::Repelling {
            if let Some(h) = &self.heart {
                let mut hold: f32 = 0.0;
                let mut inhabited: f32 = 0.0;
                let mut veins = Vec3::ZERO;
                for v in &self.veins {
                    let (a, share) = v.pull(p);
                    hold = hold.max(share);
                    veins += a;
                    if v.inhabited {
                        inhabited = inhabited.max(share);
                    }
                }
                // The heart's push already points away from it.
                out += h.push(p) * (1.0 - hold) + capped_flare(h.flare(p), veins, inhabited);
            }
        }
        for s in &self.sources {
            if s.charge != opposite
                || s.holder == Some(q.id)
                || Some(s.id) == stone.source
                || !s.may_reach(p)
            {
                continue;
            }
            let a = s.acceleration(p, q.mass);
            let size = a.length();
            if size <= 0.0 {
                continue;
            }
            out += away_from(s, p) * size;
        }
        out
    }

    /// The sources that may act at `p` (for "stone sense" and previews).
    pub fn sources_near(&self, p: Vec3) -> impl Iterator<Item = &Source> {
        self.sources
            .iter()
            .filter(move |s| s.is_active() && s.may_reach(p))
    }

    pub fn source(&self, id: SourceId) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }
}

/// Unit direction straight away from a source at `p`.
fn away_from(s: &Source, p: Vec3) -> Vec3 {
    use crate::profile::Profile;
    let d = p - s.position;
    match s.profile {
        Profile::Plane => s.axis * d.dot(s.axis).signum(),
        Profile::Line => (d - s.axis * d.dot(s.axis)).normalize_or_zero(),
        _ => d.normalize_or_zero(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vein::VeinShape;
    use glam::Quat;

    fn district() -> Field {
        Field {
            veins: vec![Vein::new(
                "floor",
                vec![VeinShape::Box {
                    center: Vec3::new(0.0, -5.0, 0.0),
                    rotation: Quat::IDENTITY,
                    half: Vec3::new(1000.0, 5.0, 1000.0),
                }],
                1.0,
            )],
            heart: Some(Heart {
                center: Vec3::new(0.0, 5000.0, 0.0),
                stripped_radius: 3000.0,
                wall_radius: 9000.0,
                push_at_stripped: 0.3,
                push_at_wall: 0.02,
                push_scale: 1.0,
                flare_g: 0.0,
            }),
            sources: vec![],
        }
    }

    #[test]
    fn the_heart_is_shielded_inside_a_hold_zone_and_fades_in() {
        let f = district();
        // In the hold zone: down is the vein alone (A7).
        let a = f.acceleration(&BodyQuery::at(Vec3::new(0.0, 10.0, 0.0)));
        assert!((a - Vec3::NEG_Y * G).length() < 1e-3, "{a}");
        // Halfway through the fade band: half vein, half heart push.
        let parts = f.parts(&BodyQuery::at(Vec3::new(0.0, 85.0, 0.0)));
        assert!((parts.vein_hold - 0.5).abs() < 1e-4);
        assert!(parts.heart.y < 0.0, "pushed away from the heart above");
        // Outside every hold zone: only the heart's push remains (R3).
        let p = f.parts(&BodyQuery::at(Vec3::new(0.0, 200.0, 0.0)));
        assert_eq!(p.veins, Vec3::ZERO);
        let r: f32 = 4800.0;
        let expect = (0.3 + (0.02 - 0.3) * (r - 3000.0) / 6000.0) * G;
        assert!((p.heart.length() - expect).abs() < 1e-3);
    }

    #[test]
    fn a_flare_breaks_through_shielding_but_a_town_keeps_its_footing() {
        // A flare would carry a town off its vein: H11 caps it there at
        // half the vein's pull, while in the open it pushes in full.
        let mut f = district();
        let heart = f.heart.as_mut().unwrap();
        // The heart below the town, so its push lifts the town off.
        heart.center = Vec3::new(0.0, -5000.0, 0.0);
        heart.flare_g = 0.8;
        let town = f.parts(&BodyQuery::at(Vec3::new(0.0, 10.0, 0.0)));
        assert!(town.heart.length() > 0.4 * G, "it gets through");
        assert!(town.heart.length() <= 0.5 * town.veins.length() + 1e-3);
        assert!(f.acceleration(&BodyQuery::at(Vec3::new(0.0, 10.0, 0.0))).y < 0.0);
        let mut wild = f.clone();
        wild.veins[0].inhabited = false;
        let free = wild.parts(&BodyQuery::at(Vec3::new(0.0, 10.0, 0.0)));
        assert!(
            (free.heart.length() - 0.8 * G).abs() < 1e-3,
            "uninhabited: full"
        );
    }

    #[test]
    fn a_held_stone_does_not_act_on_its_holder() {
        let mut f = Field::default();
        let mut s = Source::new(1, Charge::Attracting, Vec3::ZERO, 1.5, 15.0);
        s.holder = Some(42);
        f.sources.push(s);
        let mut q = BodyQuery::at(Vec3::new(0.5, 0.0, 0.0));
        q.id = 42;
        assert_eq!(f.acceleration(&q), Vec3::ZERO);
        q.id = 7;
        assert!(f.acceleration(&q).x < 0.0, "but on everything else (P8)");
    }

    #[test]
    fn lodestone_obeys_charge() {
        let mut f = Field::default();
        f.sources
            .push(Source::new(1, Charge::Attracting, Vec3::ZERO, 1.5, 15.0));
        let at = Vec3::new(5.0, 0.0, 0.0);
        let mut q = BodyQuery::at(at);
        // An open attracting stone ignores another attracting stone (A12).
        q.lodestone = Some(LodestoneBody {
            charge: Charge::Attracting,
            opening: 1.0,
            source: None,
        });
        assert_eq!(f.acceleration(&q), Vec3::ZERO);
        // An open repelling shard is pushed away as hard as the field (A13).
        q.lodestone = Some(LodestoneBody {
            charge: Charge::Repelling,
            opening: 1.0,
            source: None,
        });
        let a = f.acceleration(&q);
        let size = f.sources[0].acceleration(at, 1.0).length();
        assert!((a - Vec3::X * size).length() < 1e-4, "{a}");
        // Half open: half pulled like matter, half pushed (A14): they cancel.
        q.lodestone = Some(LodestoneBody {
            charge: Charge::Repelling,
            opening: 0.5,
            source: None,
        });
        assert!(f.acceleration(&q).length() < 1e-4);
        // Fully coated: plain matter (A11).
        q.lodestone = Some(LodestoneBody {
            charge: Charge::Repelling,
            opening: 0.0,
            source: None,
        });
        assert!((f.acceleration(&q) + Vec3::X * size).length() < 1e-4);
    }

    #[test]
    fn an_open_shard_is_pushed_off_a_vein() {
        let f = district();
        let mut q = BodyQuery::at(Vec3::new(0.0, 10.0, 0.0));
        q.lodestone = Some(LodestoneBody {
            charge: Charge::Repelling,
            opening: 1.0,
            source: None,
        });
        let a = f.acceleration(&q);
        assert!((a - Vec3::Y * G).length() < 1e-3, "{a}");
    }

    #[test]
    fn a_stone_never_acts_on_itself() {
        let mut f = Field::default();
        f.sources
            .push(Source::new(3, Charge::Repelling, Vec3::ZERO, 1.0, 10.0));
        let mut q = BodyQuery::at(Vec3::new(0.1, 0.0, 0.0));
        q.lodestone = Some(LodestoneBody {
            charge: Charge::Repelling,
            opening: 1.0,
            source: Some(SourceId(3)),
        });
        assert_eq!(f.acceleration(&q), Vec3::ZERO);
    }
}
