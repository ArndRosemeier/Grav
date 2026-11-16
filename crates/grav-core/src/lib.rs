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

/// 1 g in m/s². Pull strengths in the design are given in g.
pub const G: f32 = 9.81;
