//! Grav's rules as plain data: the attraction field, stones and their
//! coating, building bonds and the hero's body rules.
//!
//! Nothing here knows about a renderer, a window or a physics engine. The
//! same inputs give the same outputs on every machine, so a game server can
//! own sources and large debris later (design doc, "Multiplayer").
//!
//! Rule numbers in comments (A1, S9, W3, P6 ...) refer to the design
//! document "Grav: world and concept design". Numbers to tune live in
//! [`Tuning`], loaded from JSON, never as constants in game code.

pub mod bonds;
pub mod field;
pub mod hero;
pub mod profile;
pub mod source;
pub mod stone;
pub mod tuning;
pub mod vein;
pub mod weather;

pub use bonds::{BondClass, PieceId, PieceStage, Structure, StructureStep};
pub use field::{BodyQuery, Field, FieldParts, Heart, LodestoneBody};
pub use hero::{
    can_walk, climb_slope_deg, fall_damage, footing_holds, grip, jump_speed, press_and_drag,
    slide_step, walk_accel, walk_speed_in, Movement, ROCK_FRICTION,
};

pub use profile::{Profile, Selectivity, TimeDial};
pub use source::{face_of, Charge, Source, SourceId, ALL_FACES};
pub use stone::{is_round, BoxState, Face, Flight, Program, Stone, StoneSpec};
pub use tuning::Tuning;

pub use vein::{Vein, VeinShape};
pub use weather::{Flare, Sky, Weather, WeatherTuning};

pub use glam::{Quat, Vec3};

/// The rotation that turns +Y to `up` and +Z to `front`: how a thing stands
/// on ground whose down is `-up`, facing `front`. Both unit and square to
/// each other.
pub fn facing(up: Vec3, front: Vec3) -> Quat {
    Quat::from_mat3(&glam::Mat3::from_cols(up.cross(front), up, front))
}

/// The unit direction along the ground (square to `up`) closest to `v`;
/// any such direction when `v` points straight up or down.
pub fn along_ground(v: Vec3, up: Vec3) -> Vec3 {
    (v - up * v.dot(up)).normalize_or(up.any_orthonormal_vector())
}

/// Standing on `up`, facing as near `toward` as the ground allows.
pub fn upright(up: Vec3, toward: Vec3) -> Quat {
    facing(up, along_ground(toward, up))
}

/// What "1 g" means in this world by default, in m/s²: half of Earth's
/// (Arnd, 2026-11-16: walking felt too heavy).
pub const DEFAULT_ONE_G: f32 = EARTH_G * 0.5;

static ONE_G_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// What "1 g" means in this world, in m/s². Pull strengths in the design
/// are given in g, and every pull, press and fall scales with this. The
/// player can change it in the settings (`set_one_g`) to find what plays
/// well; everything reads it here, so nothing keeps its own copy.
pub fn one_g() -> f32 {
    let bits = ONE_G_BITS.load(std::sync::atomic::Ordering::Relaxed);
    if bits == 0 {
        DEFAULT_ONE_G
    } else {
        f32::from_bits(bits)
    }
}

/// Set what "1 g" means (m/s², kept within 0.5 to 20).
pub fn set_one_g(metres_per_s2: f32) {
    let v = if metres_per_s2.is_finite() {
        metres_per_s2.clamp(0.5, 20.0)
    } else {
        DEFAULT_ONE_G
    };
    ONE_G_BITS.store(v.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

/// Earth's gravity, m/s²: what bodies are built for. The hero's legs push
/// off as hard as on Earth, so in a lighter g he jumps higher.
pub const EARTH_G: f32 = 9.81;
