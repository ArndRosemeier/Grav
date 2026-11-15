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
pub mod coating;
pub mod field;
pub mod hero;
pub mod profile;
pub mod source;
pub mod stone;
pub mod tuning;
pub mod vein;

pub use bonds::{BondClass, PieceId, PieceStage, Structure, StructureStep};
pub use coating::{Coating, CoatingError};
pub use field::{BodyQuery, Field, FieldParts, Heart, LodestoneBody};
pub use hero::{
    can_walk, fall_damage, jump_speed, press_and_drag, slide_accel, slide_brake, walk_accel,
    walk_speed_in, Movement,
};

pub use profile::{Profile, Selectivity, TimeDial};
pub use source::{Charge, Source, SourceId};
pub use stone::{Placement, Stone, StoneSpec};
pub use tuning::Tuning;

pub use vein::{Vein, VeinShape};

pub use glam::{Quat, Vec3};

/// 1 g in m/s². Pull strengths in the design are given in g.
pub const G: f32 = 9.81;
