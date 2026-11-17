//! A hand stone in its coated box (S14 to S20, Arnd 2026-11-16).
//!
//! The coating shields from attraction, so a closed box is weightless and
//! makes no field: thrown, it flies straight and only air slows it. Each of
//! its six faces opens on a hinge at its own time after the box has come
//! to rest, and the pull spills out of the open sides only. The guild's cut
//! stones feel no other lodestone, so an open box hangs where it stopped.
//! A last timer bares a little plain mass: the box obeys the field again,
//! comes down, and the hero picks it up where it lies.

use crate::profile::{Profile, Selectivity, TimeDial};
use crate::source::{Charge, Source, SourceId, ALL_FACES};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

/// What a stone is: its charge and its dressing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoneSpec {
    pub name: String,
    pub charge: Charge,
    /// Peak magnitude in g with a face open.
    pub max_g: f32,
    pub radius: f32,
    pub profile: Profile,
    #[serde(default)]
    pub time: TimeDial,
    #[serde(default)]
    pub selectivity: Selectivity,
    /// Adjuster slots (1 common, 2 guild, 3 rare loot; 0 for the shard).
    #[serde(default)]
    pub slots: u8,
}

/// A face of the box. The box's frame comes from the hand that threw it:
/// the front faces away from the thrower, up is the thrower's up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Face {
    Front,
    Back,
    Left,
    Right,
    Up,
    Down,
}

impl Face {
    /// In bit order of [`Source::faces`].
    pub const ALL: [Face; 6] = [
        Face::Front,
        Face::Back,
        Face::Left,
        Face::Right,
        Face::Up,
        Face::Down,
    ];

    pub fn index(self) -> usize {
        Face::ALL.iter().position(|f| *f == self).expect("listed")
    }

    /// Outward normal in the box's frame (+z front, +y up, +x left).
    pub fn normal(self) -> Vec3 {
        match self {
            Face::Front => Vec3::Z,
            Face::Back => Vec3::NEG_Z,
            Face::Left => Vec3::X,
            Face::Right => Vec3::NEG_X,
            Face::Up => Vec3::Y,
            Face::Down => Vec3::NEG_Y,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Face::Front => "Front",
            Face::Back => "Back",
            Face::Left => "Left",
            Face::Right => "Right",
            Face::Up => "Up",
            Face::Down => "Down",
        }
    }
}

/// What the box's clockwork does once it has come to rest (S15, S16).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Program {
    /// Seconds after coming to rest that each face opens; `None` stays
    /// shut. Indexed like [`Face::ALL`]. A face opens fully and stays open.
    pub faces: [Option<f32>; 6],
    /// Seconds after coming to rest that plain mass is bared and the box
    /// comes down.
    pub drop_after: f32,
}

impl Default for Program {
    /// The round field: every face opens a second after the box stops,
    /// and it comes down after 20 s.
    fn default() -> Self {
        Self {
            faces: [Some(1.0); 6],
            drop_after: 20.0,
        }
    }
}

impl Program {
    /// Only these faces, all at `after` seconds.
    pub fn only(faces: &[Face], after: f32, drop_after: f32) -> Self {
        let mut p = Self {
            faces: [None; 6],
            drop_after,
        };
        for f in faces {
            p.faces[f.index()] = Some(after);
        }
        p
    }

    pub fn armed(&self) -> usize {
        self.faces.iter().filter(|f| f.is_some()).count()
    }
}

/// Where a box is.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum BoxState {
    /// With the hero, shut.
    Ready,
    /// Thrown and shut: weightless, slowed only by air.
    Flying { at: Vec3, velocity: Vec3 },
    /// Hanging still where it stopped; its clocks run.
    Resting { at: Vec3 },
    /// Plain mass bared: the field carries it like any body (the game's
    /// physics writes `at`) until the hero picks it up.
    Dropped { at: Vec3 },
}

/// How a thrown box flies (numbers live in [`crate::Tuning`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flight {
    /// Air drag: speed falls by this share per second (linear drag, so a
    /// throw at speed `v` stops `(v - rest_speed) / drag` metres away).
    pub drag: f32,
    /// Below this speed the box stops and hangs.
    pub rest_speed: f32,
    /// The throw's range: shortest, longest, and how much one click adds
    /// or takes away, m.
    pub min_range: f32,
    pub max_range: f32,
    pub range_step: f32,
    /// Holding the button keeps changing the range at this many m/s.
    pub range_rate: f32,
    /// Half the box's size, m.
    pub half_size: f32,
    /// How close the hero must come to pick up a dropped box, m.
    pub pickup_reach: f32,
}

impl Default for Flight {
    fn default() -> Self {
        Self {
            drag: 1.5,
            rest_speed: 0.4,
            min_range: 2.0,
            max_range: 30.0,
            range_step: 1.0,
            range_rate: 8.0,
            half_size: 0.18,
            pickup_reach: 1.8,
        }
    }
}

impl Flight {
    /// How far a box thrown at `speed` flies before it hangs.
    pub fn range(&self, speed: f32) -> f32 {
        ((speed - self.rest_speed) / self.drag.max(1e-3)).max(0.0)
    }

    /// The throw speed that carries a box `range` metres.
    pub fn speed_for(&self, range: f32) -> f32 {
        range.max(0.0) * self.drag.max(1e-3) + self.rest_speed
    }

    /// Seconds a box thrown at `speed` flies before it hangs.
    pub fn duration(&self, speed: f32) -> f32 {
        if speed <= self.rest_speed {
            return 0.0;
        }
        (speed / self.rest_speed).ln() / self.drag.max(1e-3)
    }
}

/// A stone the hero owns, in its box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stone {
    pub id: SourceId,
    pub spec: StoneSpec,
    pub program: Program,
    pub state: BoxState,
    /// The box's orientation (set by the throw).
    pub frame: Quat,
    /// Which faces are open now.
    pub open: [bool; 6],
    /// Seconds since the box came to rest.
    pub since_rest: f32,
}

impl Stone {
    pub fn new(id: u32, spec: StoneSpec) -> Self {
        Self {
            id: SourceId(id),
            spec,
            program: Program::default(),
            state: BoxState::Ready,
            frame: Quat::IDENTITY,
            open: [false; 6],
            since_rest: 0.0,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.state == BoxState::Ready
    }

    /// Where the box is, unless the hero has it.
    pub fn position(&self) -> Option<Vec3> {
        match self.state {
            BoxState::Ready => None,
            BoxState::Flying { at, .. } | BoxState::Resting { at } | BoxState::Dropped { at } => {
                Some(at)
            }
        }
    }

    /// Throw the box from `from` at `velocity`, oriented by `frame`.
    /// Returns false when the hero does not have it.
    pub fn throw(&mut self, from: Vec3, velocity: Vec3, frame: Quat) -> bool {
        if !self.is_ready() {
            return false;
        }
        self.state = BoxState::Flying { at: from, velocity };
        self.frame = frame;
        self.open = [false; 6];
        self.since_rest = 0.0;
        true
    }

    /// One step of flight: returns the segment flown (for the game to test
    /// against the world), and stops the box once air has slowed it.
    pub fn fly(&mut self, flight: &Flight, dt: f32) -> Option<(Vec3, Vec3)> {
        let BoxState::Flying { at, velocity } = self.state else {
            return None;
        };
        // Linear drag, solved exactly: v(t) = v0 e^(-k t).
        let k = flight.drag.max(1e-3);
        let decay = crate::drag_keep(k, dt);
        let to = at + velocity * (1.0 - decay) / k;
        let velocity = velocity * decay;
        self.state = if velocity.length() < flight.rest_speed {
            BoxState::Resting { at: to }
        } else {
            BoxState::Flying { at: to, velocity }
        };
        Some((at, to))
    }

    /// Stop the box at `at` (it hit something, or air stopped it).
    pub fn rest_at(&mut self, at: Vec3) {
        if matches!(
            self.state,
            BoxState::Flying { .. } | BoxState::Resting { .. }
        ) {
            self.state = BoxState::Resting { at };
        }
    }

    /// Run the clocks of a box at rest: open faces, bare mass.
    pub fn tick(&mut self, dt: f32) {
        if !matches!(
            self.state,
            BoxState::Resting { .. } | BoxState::Dropped { .. }
        ) {
            return;
        }
        self.since_rest += dt;
        for (open, at) in self.open.iter_mut().zip(self.program.faces) {
            if at.is_some_and(|t| self.since_rest >= t) {
                *open = true;
            }
        }
        if let BoxState::Resting { at } = self.state {
            if self.since_rest >= self.program.drop_after {
                self.state = BoxState::Dropped { at };
            }
        }
    }

    /// Seconds until face `i` opens (None: it stays shut or is open).
    pub fn face_countdown(&self, i: usize) -> Option<f32> {
        let at = self.program.faces[i]?;
        (!self.open[i]).then(|| (at - self.since_rest).max(0.0))
    }

    /// Seconds until the box comes down (None once it has).
    pub fn drop_countdown(&self) -> Option<f32> {
        matches!(
            self.state,
            BoxState::Resting { .. } | BoxState::Flying { .. }
        )
        .then(|| (self.program.drop_after - self.since_rest).max(0.0))
    }

    /// The physics carries a dropped box: tell the stone where it is.
    pub fn moved_to(&mut self, at: Vec3) {
        if let BoxState::Dropped { .. } = self.state {
            self.state = BoxState::Dropped { at };
        }
    }

    /// The hero takes a dropped box back: shut, clocks rewound (S19).
    /// Returns false when it has not come down.
    pub fn pick_up(&mut self) -> bool {
        if !matches!(self.state, BoxState::Dropped { .. }) {
            return false;
        }
        self.state = BoxState::Ready;
        self.open = [false; 6];
        self.since_rest = 0.0;
        true
    }

    /// Which faces are open now, as a [`Source::faces`] mask.
    pub fn open_mask(&self) -> u8 {
        self.open
            .iter()
            .enumerate()
            .filter(|(_, o)| **o)
            .fold(0, |m, (i, _)| m | 1 << i)
    }

    /// The faces this box's program will open.
    pub fn programmed_mask(&self) -> u8 {
        self.program
            .faces
            .iter()
            .enumerate()
            .filter(|(_, f)| f.is_some())
            .fold(0, |m, (i, _)| m | 1 << i)
    }

    /// The field this box makes now. `body` is the box's own body once it
    /// has come down, which its field does not act on.
    pub fn source(&self, body: Option<u64>) -> Option<Source> {
        let at = match self.state {
            BoxState::Resting { at } | BoxState::Dropped { at } => at,
            _ => return None,
        };
        let faces = self.open_mask();
        (faces != 0).then(|| self.source_at(at, self.frame, faces, body))
    }

    /// The field this box would make at `at` with `faces` open (for the
    /// throw preview).
    pub fn source_at(&self, at: Vec3, frame: Quat, faces: u8, body: Option<u64>) -> Source {
        Source {
            id: self.id,
            charge: self.spec.charge,
            position: at,
            axis: frame * Vec3::Y,
            max_g: self.spec.max_g,
            opening: 1.0,
            radius: self.spec.radius,
            profile: self.spec.profile,
            time: self.spec.time,
            selectivity: self.spec.selectivity,
            open_seconds: self.since_rest,
            holder: body,
            faces,
            frame,
        }
    }
}

/// Whether every face is open (the round field).
pub fn is_round(mask: u8) -> bool {
    mask == ALL_FACES
}

#[cfg(test)]
mod tests {
    use super::*;

    fn common() -> StoneSpec {
        StoneSpec {
            name: "common".into(),
            charge: Charge::Attracting,
            max_g: 1.5,
            radius: 15.0,
            profile: Profile::Core,
            time: TimeDial::Constant,
            selectivity: Selectivity::All,
            slots: 1,
        }
    }

    #[test]
    fn a_thrown_box_flies_straight_and_stops_where_the_range_says() {
        let flight = Flight::default();
        let mut s = Stone::new(1, common());
        let speed = flight.speed_for(12.0);
        assert!(s.throw(Vec3::ZERO, Vec3::X * speed, Quat::IDENTITY));
        assert!(s.source(None).is_none(), "shut in flight");
        let mut t = 0.0;
        while s.fly(&flight, 1.0 / 60.0).is_some() {
            t += 1.0 / 60.0;
            assert!(t < 10.0);
        }
        let at = s.position().unwrap();
        assert!(at.y.abs() < 1e-6 && at.z.abs() < 1e-6, "{at}");
        assert!(
            (at.x - flight.range(speed)).abs() < 0.05,
            "{at} {}",
            flight.range(speed)
        );
        assert!((t - flight.duration(speed)).abs() < 0.05);
        assert!((at.x - 12.0).abs() < 0.05);
    }

    #[test]
    fn faces_open_on_their_clocks_and_the_box_drops() {
        let mut s = Stone::new(1, common());
        s.program = Program::only(&[Face::Front], 2.0, 5.0);
        s.throw(Vec3::ZERO, Vec3::ZERO, Quat::IDENTITY);
        s.rest_at(Vec3::new(0.0, 3.0, 0.0));
        s.tick(1.0);
        assert!(s.source(None).is_none());
        assert!((s.face_countdown(0).unwrap() - 1.0).abs() < 1e-5);
        s.tick(1.5);
        let src = s.source(None).expect("front open");
        assert_eq!(src.faces, 1);
        assert!(!s.pick_up(), "still hanging");
        s.tick(3.0);
        assert!(matches!(s.state, BoxState::Dropped { .. }));
        assert!(s.pick_up());
        assert!(s.is_ready() && s.source(None).is_none());
    }

    #[test]
    fn the_front_faces_away_from_the_thrower() {
        // A box thrown towards +x by someone standing up +y: its front
        // field pulls only from the far side.
        let mut s = Stone::new(1, common());
        s.program = Program::only(&[Face::Front], 0.0, 99.0);
        let front = Vec3::X;
        let up = Vec3::Y;
        let frame = crate::facing(up, front);
        s.throw(Vec3::ZERO, Vec3::ZERO, frame);
        s.rest_at(Vec3::ZERO);
        s.tick(0.1);
        let src = s.source(None).unwrap();
        assert!(src.acceleration(Vec3::new(5.0, 0.5, 0.0), 1.0).x < -1.0);
        assert_eq!(src.acceleration(Vec3::new(-5.0, 0.0, 0.0), 1.0), Vec3::ZERO);
    }
}
