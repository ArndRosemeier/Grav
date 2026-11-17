//! The challenge: the self-contained unit of play (design doc,
//! "Challenge"): a place, an objective, constraints and a reward, scored
//! on collateral damage, what the hero spent, and time.
//!
//! A challenge is plain data and loads from JSON. The game watches the
//! world each frame and hands what it saw to [`Attempt::step`] as an
//! [`Observed`]; everything that decides winning, failing and the score is
//! here, so a server can judge an attempt the same way.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// What the hero must bring about.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Objective {
    /// Get the hero to a place.
    Reach { target: [f32; 3], radius: f32 },
    /// Bring a structure down: at least `share` of its pieces broken loose.
    Demolish { structure: String, share: f32 },
    /// Move a mass (a named loose thing) to a place.
    Haul {
        cargo: String,
        to: [f32; 3],
        radius: f32,
    },
    /// Keep a thing within `radius` of `anchor` for `seconds` (while a
    /// flare or a weak vein tries to carry it off).
    Hold {
        thing: String,
        anchor: [f32; 3],
        radius: f32,
        seconds: f32,
    },
    /// Gather `count` loose pieces in a place (a plug, a dam, a bridge).
    Build {
        zone: [f32; 3],
        radius: f32,
        count: u32,
    },
    /// Bring someone out to a safe place, alive.
    Rescue {
        who: String,
        to: [f32; 3],
        radius: f32,
    },
    /// Defeat opponents: knock `foes` of them off their feet.
    Fight { foes: u32 },
}

/// What the hero must not do, or not exceed. A broken constraint fails
/// the attempt at once.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Constraints {
    /// Seconds to do it in.
    pub time_limit_s: Option<f32>,
    /// Stones (boxes) the hero may open.
    pub max_boxes: Option<u32>,
    /// Collateral damage allowed, in collateral points (see [`Collateral::points`]).
    pub max_collateral: Option<f32>,
    /// Structures that must stay standing (nothing of them may break).
    pub spare: Vec<String>,
    /// Nobody may be hurt.
    pub no_harm: bool,
}

/// What the hero gets for it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reward {
    pub coin: u32,
    /// Faction standing changes, e.g. ("dwarves", 5).
    pub standing: Vec<(String, i32)>,
}

/// What a clean, quick run looks like: scoring compares against it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Par {
    pub time_s: f32,
    pub boxes: u32,
    pub collateral: f32,
}

impl Default for Par {
    fn default() -> Self {
        Self {
            time_s: 60.0,
            boxes: 2,
            collateral: 0.0,
        }
    }
}

/// One challenge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Challenge {
    pub id: String,
    pub title: String,
    /// What the player is told.
    pub brief: String,
    /// Where the hero starts (feet), and facing which way.
    pub start: [f32; 3],
    #[serde(default)]
    pub start_yaw_deg: f32,
    pub objective: Objective,
    #[serde(default)]
    pub constraints: Constraints,
    #[serde(default)]
    pub reward: Reward,
    #[serde(default)]
    pub par: Par,
}

impl Challenge {
    pub fn start(&self) -> Vec3 {
        Vec3::from(self.start)
    }

    /// Where the objective is, to point the player at it.
    pub fn goal(&self) -> Option<Vec3> {
        match &self.objective {
            Objective::Reach { target, .. } => Some(Vec3::from(*target)),
            Objective::Haul { to, .. } => Some(Vec3::from(*to)),
            Objective::Hold { anchor, .. } => Some(Vec3::from(*anchor)),
            Objective::Build { zone, .. } => Some(Vec3::from(*zone)),
            Objective::Rescue { to, .. } => Some(Vec3::from(*to)),
            Objective::Demolish { .. } | Objective::Fight { .. } => None,
        }
    }

    /// A list of challenges from JSON.
    pub fn list_from_json(json: &str) -> Result<Vec<Challenge>, String> {
        let list: Vec<Challenge> = serde_json::from_str(json).map_err(|e| e.to_string())?;
        for c in &list {
            c.check()?;
        }
        Ok(list)
    }

    /// Is this challenge well formed?
    pub fn check(&self) -> Result<(), String> {
        let bad = |what: &str| Err(format!("challenge {}: {what}", self.id));
        let finite = |v: [f32; 3]| v.iter().all(|x| x.is_finite());
        if !finite(self.start) {
            return bad("start is not a finite point");
        }
        let place = |v: [f32; 3], r: f32| finite(v) && r > 0.0;
        match &self.objective {
            Objective::Reach { target, radius } => {
                if !finite(*target) || *radius <= 0.0 || radius.is_nan() {
                    return bad("reach needs a finite target and a radius above 0");
                }
            }
            Objective::Demolish { structure, share } => {
                if structure.is_empty() || !(0.0..=1.0).contains(share) || *share == 0.0 {
                    return bad("demolish needs a structure and a share in (0, 1]");
                }
                if self.constraints.spare.contains(structure) {
                    return bad("the structure to demolish is also to be spared");
                }
            }
            Objective::Haul { cargo, to, radius } => {
                if cargo.is_empty() || !finite(*to) || *radius <= 0.0 || radius.is_nan() {
                    return bad("haul needs cargo, a finite place and a radius above 0");
                }
            }
            Objective::Hold {
                thing,
                anchor,
                radius,
                seconds,
            } => {
                if thing.is_empty()
                    || !place(*anchor, *radius)
                    || seconds.is_nan()
                    || *seconds <= 0.0
                {
                    return bad(
                        "hold needs a thing, a finite anchor, a radius and seconds above 0",
                    );
                }
                if self.constraints.time_limit_s.is_some_and(|t| t < *seconds) {
                    return bad("hold lasts longer than its time limit");
                }
            }
            Objective::Build {
                zone,
                radius,
                count,
            } => {
                if !place(*zone, *radius) || *count == 0 {
                    return bad("build needs a finite zone, a radius and a count above 0");
                }
            }
            Objective::Rescue { who, to, radius } => {
                if who.is_empty() || !place(*to, *radius) {
                    return bad("rescue needs someone, a finite place and a radius above 0");
                }
            }
            Objective::Fight { foes } => {
                if *foes == 0 {
                    return bad("fight needs at least one foe");
                }
            }
        }
        if self
            .constraints
            .time_limit_s
            .is_some_and(|t| t.is_nan() || t <= 0.0)
        {
            return bad("time limit must be above 0");
        }
        if self.par.time_s.is_nan() || self.par.time_s <= 0.0 {
            return bad("par time must be above 0");
        }
        Ok(())
    }
}

/// Damage done on the way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Collateral {
    /// Building pieces broken loose that were not the objective.
    pub pieces: u32,
    /// Goods knocked loose.
    pub goods: u32,
    /// People hurt or carried off (F6).
    pub people: u32,
}

impl Collateral {
    /// One number: a hurt person weighs as much as a small house.
    pub fn points(&self) -> f32 {
        self.pieces as f32 + 2.0 * self.goods as f32 + 25.0 * self.people as f32
    }
}

/// What the game saw this frame, totals since the attempt began.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Observed {
    pub hero: Vec3,
    /// Where the cargo is, for a haul; the thing held, for a hold; the
    /// one to rescue, for a rescue.
    pub cargo: Option<Vec3>,
    /// The thing held or the one to rescue is gone for good.
    pub lost: bool,
    /// Loose pieces at rest in a build's zone.
    pub in_zone: u32,
    /// Opponents knocked off their feet.
    pub foes_down: u32,
    /// The hero is down (no health left).
    pub hero_down: bool,
    /// Share of the target structure's pieces broken loose, 0..1.
    pub target_down: f32,
    /// Pieces broken in structures that are to be spared.
    pub spared_broken: u32,
    pub collateral: Collateral,
    /// Stones the hero has opened since the start.
    pub boxes: u32,
}

/// How an attempt stands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    Running,
    Won,
    Failed(String),
}

/// The score of a won attempt.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// 0 to 1000.
    pub points: u32,
    /// 1 to 3.
    pub stars: u8,
    /// Each part 0..1: time, boxes, collateral.
    pub time: f32,
    pub boxes: f32,
    pub care: f32,
}

/// One go at a challenge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attempt {
    pub elapsed_s: f32,
    pub boxes: u32,
    pub collateral: Collateral,
    pub outcome: Outcome,
}

impl Default for Attempt {
    fn default() -> Self {
        Self::new()
    }
}

impl Attempt {
    pub fn new() -> Self {
        Self {
            elapsed_s: 0.0,
            boxes: 0,
            collateral: Collateral::default(),
            outcome: Outcome::Running,
        }
    }

    /// Advance by `dt` seconds with what the game saw. Once won or failed,
    /// the attempt stays so.
    pub fn step(&mut self, c: &Challenge, dt: f32, seen: &Observed) -> &Outcome {
        if self.outcome != Outcome::Running {
            return &self.outcome;
        }
        self.elapsed_s += dt.max(0.0);
        self.boxes = seen.boxes;
        self.collateral = seen.collateral;
        let k = &c.constraints;
        let fail = if k.spare.is_empty() || seen.spared_broken == 0 {
            None
        } else {
            Some("something that had to stand came down".to_string())
        };
        let fail = fail
            .or_else(|| {
                (k.no_harm && seen.collateral.people > 0).then(|| "someone was hurt".to_string())
            })
            .or_else(|| {
                k.max_boxes
                    .filter(|&m| seen.boxes > m)
                    .map(|m| format!("more than {m} stones opened"))
            })
            .or_else(|| {
                k.max_collateral
                    .filter(|&m| seen.collateral.points() > m)
                    .map(|_| "too much damage around".to_string())
            });
        let fail = fail.or_else(|| seen.hero_down.then(|| "you went down".to_string()));
        let fail = fail.or_else(|| match &c.objective {
            Objective::Rescue { .. } if seen.lost => Some("they were carried away".to_string()),
            Objective::Hold { anchor, radius, .. } => {
                let out = seen.lost
                    || seen
                        .cargo
                        .is_none_or(|p| p.distance(Vec3::from(*anchor)) > *radius);
                out.then(|| "it was carried off".to_string())
            }
            _ => None,
        });
        if let Some(why) = fail {
            self.outcome = Outcome::Failed(why);
            return &self.outcome;
        }
        let done = match &c.objective {
            Objective::Reach { target, radius } => {
                seen.hero.distance(Vec3::from(*target)) <= *radius
            }
            Objective::Demolish { share, .. } => seen.target_down >= *share,
            Objective::Haul { to, radius, .. } | Objective::Rescue { to, radius, .. } => seen
                .cargo
                .is_some_and(|p| p.distance(Vec3::from(*to)) <= *radius),
            Objective::Hold { seconds, .. } => self.elapsed_s >= *seconds,
            Objective::Build { count, .. } => seen.in_zone >= *count,
            Objective::Fight { foes } => seen.foes_down >= *foes,
        };
        if done {
            self.outcome = Outcome::Won;
        } else if k.time_limit_s.is_some_and(|t| self.elapsed_s > t) {
            self.outcome = Outcome::Failed("out of time".to_string());
        }
        &self.outcome
    }

    /// Seconds left, if the challenge has a limit.
    pub fn time_left(&self, c: &Challenge) -> Option<f32> {
        c.constraints
            .time_limit_s
            .map(|t| (t - self.elapsed_s).max(0.0))
    }

    /// The score, once won: each part is 1 at par or better and falls off
    /// beyond it; the points weigh care heaviest.
    pub fn score(&self, c: &Challenge) -> Option<Score> {
        if self.outcome != Outcome::Won {
            return None;
        }
        let past = |got: f32, par: f32, slack: f32| {
            if got <= par {
                1.0
            } else {
                (1.0 - (got - par) / slack.max(1e-3)).clamp(0.0, 1.0)
            }
        };
        let time = past(self.elapsed_s, c.par.time_s, c.par.time_s * 2.0);
        let boxes = past(self.boxes as f32, c.par.boxes as f32, 4.0);
        let care = past(self.collateral.points(), c.par.collateral, 60.0);
        let points = (1000.0 * (0.25 * time + 0.25 * boxes + 0.5 * care)).round() as u32;
        let stars = if points >= 900 {
            3
        } else if points >= 600 {
            2
        } else {
            1
        };
        Some(Score {
            points,
            stars,
            time,
            boxes,
            care,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reach() -> Challenge {
        Challenge {
            id: "r".into(),
            title: "Up".into(),
            brief: "Get up there".into(),
            start: [0.0; 3],
            start_yaw_deg: 0.0,
            objective: Objective::Reach {
                target: [0.0, 10.0, 0.0],
                radius: 1.0,
            },
            constraints: Constraints {
                time_limit_s: Some(30.0),
                ..Default::default()
            },
            reward: Reward::default(),
            par: Par {
                time_s: 10.0,
                boxes: 1,
                collateral: 0.0,
            },
        }
    }

    // Guards the win and fail rules every challenge relies on.
    #[test]
    fn a_challenge_is_won_failed_and_scored_by_its_rules() {
        let c = reach();
        let mut a = Attempt::new();
        let mut seen = Observed::default();
        assert_eq!(a.step(&c, 1.0, &seen), &Outcome::Running);
        seen.hero = Vec3::new(0.0, 9.5, 0.0);
        assert_eq!(a.step(&c, 1.0, &seen), &Outcome::Won);
        let s = a.score(&c).unwrap();
        assert_eq!((s.points, s.stars), (1000, 3));
        // Once won, it stays won.
        seen.collateral.people = 3;
        assert_eq!(a.step(&c, 1.0, &seen), &Outcome::Won);

        let mut late = Attempt::new();
        let far = Observed::default();
        for _ in 0..31 {
            late.step(&c, 1.0, &far);
        }
        assert_eq!(late.outcome, Outcome::Failed("out of time".into()));
        assert!(late.score(&c).is_none());

        let mut c2 = reach();
        c2.constraints.no_harm = true;
        let mut hurt = Attempt::new();
        let seen = Observed {
            collateral: Collateral {
                people: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(matches!(hurt.step(&c2, 0.1, &seen), Outcome::Failed(_)));
    }

    #[test]
    #[ignore = "full"]
    fn hold_build_rescue_and_fight_follow_their_rules() {
        let mut c = reach();
        c.objective = Objective::Hold {
            thing: "cask".into(),
            anchor: [0.0; 3],
            radius: 2.0,
            seconds: 5.0,
        };
        let at_home = Observed {
            cargo: Some(Vec3::new(1.0, 0.0, 0.0)),
            ..Default::default()
        };
        let mut a = Attempt::new();
        assert_eq!(a.step(&c, 3.0, &at_home), &Outcome::Running);
        assert_eq!(a.step(&c, 3.0, &at_home), &Outcome::Won);
        let mut blown = Attempt::new();
        let off = Observed {
            cargo: Some(Vec3::new(3.0, 0.0, 0.0)),
            ..Default::default()
        };
        assert!(matches!(blown.step(&c, 1.0, &off), Outcome::Failed(_)));

        c.objective = Objective::Build {
            zone: [0.0; 3],
            radius: 3.0,
            count: 4,
        };
        let mut b = Attempt::new();
        let mut seen = Observed {
            in_zone: 3,
            ..Default::default()
        };
        assert_eq!(b.step(&c, 1.0, &seen), &Outcome::Running);
        seen.in_zone = 4;
        assert_eq!(b.step(&c, 1.0, &seen), &Outcome::Won);

        c.objective = Objective::Rescue {
            who: "miner".into(),
            to: [0.0, 0.0, 10.0],
            radius: 2.0,
        };
        let mut r = Attempt::new();
        let gone = Observed {
            lost: true,
            ..Default::default()
        };
        assert!(matches!(r.step(&c, 1.0, &gone), Outcome::Failed(_)));
        let mut r = Attempt::new();
        let safe = Observed {
            cargo: Some(Vec3::new(0.0, 0.0, 9.0)),
            ..Default::default()
        };
        assert_eq!(r.step(&c, 1.0, &safe), &Outcome::Won);

        c.objective = Objective::Fight { foes: 3 };
        let mut f = Attempt::new();
        let down = Observed {
            hero_down: true,
            ..Default::default()
        };
        assert!(matches!(f.step(&c, 1.0, &down), Outcome::Failed(_)));
        let mut f = Attempt::new();
        let won = Observed {
            foes_down: 3,
            ..Default::default()
        };
        assert_eq!(f.step(&c, 1.0, &won), &Outcome::Won);
    }

    #[test]
    #[ignore = "full"]
    fn damage_and_extra_stones_lower_the_score() {
        let c = reach();
        let won = |boxes: u32, pieces: u32, secs: f32| {
            let mut a = Attempt::new();
            let seen = Observed {
                hero: Vec3::new(0.0, 10.0, 0.0),
                boxes,
                collateral: Collateral {
                    pieces,
                    ..Default::default()
                },
                ..Default::default()
            };
            a.step(&c, secs, &seen);
            a.score(&c).unwrap().points
        };
        let clean = won(1, 0, 5.0);
        assert!(won(3, 0, 5.0) < clean);
        assert!(
            won(1, 20, 5.0) < won(3, 0, 5.0),
            "care weighs more than stones"
        );
        assert!(won(1, 0, 25.0) < clean);
    }

    #[test]
    #[ignore = "full"]
    fn challenges_load_from_json_and_bad_ones_are_refused() {
        let json = r#"[
            {"id": "d", "title": "Down", "brief": "Bring the shack down", "start": [0, 0, 0],
             "objective": {"type": "demolish", "structure": "shack", "share": 0.6},
             "constraints": {"spare": ["hall"], "no_harm": true}},
            {"id": "h", "title": "Carry", "brief": "Bring the crate", "start": [1, 2, 3],
             "objective": {"type": "haul", "cargo": "crate", "to": [5, 0, 0], "radius": 2}}
        ]"#;
        let list = Challenge::list_from_json(json).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].goal(), Some(Vec3::new(5.0, 0.0, 0.0)));
        let bad = r#"[{"id": "x", "title": "", "brief": "", "start": [0, 0, 0],
            "objective": {"type": "demolish", "structure": "hall", "share": 0.5},
            "constraints": {"spare": ["hall"]}}]"#;
        assert!(Challenge::list_from_json(bad).is_err());
    }
}
