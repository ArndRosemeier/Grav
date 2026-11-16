//! Measuring the frame rate (Q3: speed changes are measured).
//!
//! `GRAV_FRAME_TIMES=1` shows a read-out while playing (the window title
//! has the frame rate anyway). `GRAV_MEASURE=1` (measure.bat)
//! runs the measuring tour instead of the game: a few fixed views, each
//! with everything drawn and then with one more part taken away, and
//! writes what each part costs to `measure.txt`.

use crate::grass::Grass;
use crate::shots;
use crate::space::Scenery;
use engine::prelude::*;
use std::collections::VecDeque;
use std::time::Instant;

/// What is taken away, one after the other.
const STEPS: [&str; 5] = ["everything", "grass", "plants", "buildings", "far cavern"];

pub struct Meter {
    last: Option<Instant>,
    /// The last frames' times in milliseconds.
    recent: VecDeque<f32>,
    shown: bool,
    run: Option<Run>,
    result: Vec<String>,
}

struct Run {
    views: Vec<shots::Spot>,
    /// Which step and view is being measured, and since when.
    step: usize,
    view: usize,
    seconds: f32,
    frames: u32,
    sum: f32,
    worst: f32,
    /// Average and worst milliseconds per step and view.
    table: Vec<Vec<(f32, f32)>>,
    settle: f32,
    hold: f32,
}

impl Meter {
    pub fn new() -> Self {
        let seconds = |key: &str, default: f32| {
            std::env::var(key)
                .ok()
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(default)
        };
        let run = std::env::var("GRAV_MEASURE").is_ok().then(|| Run {
            views: shots::measure_views(),
            step: 0,
            view: 0,
            seconds: 0.0,
            frames: 0,
            sum: 0.0,
            worst: 0.0,
            table: vec![Vec::new()],
            settle: seconds("GRAV_MEASURE_SETTLE", 1.5),
            hold: seconds("GRAV_MEASURE_HOLD", 4.0),
        });
        Self {
            last: None,
            recent: VecDeque::new(),
            shown: std::env::var("GRAV_FRAME_TIMES").is_ok(),
            run,
            result: Vec::new(),
        }
    }

    /// True while the measuring tour runs or shows its result (the game
    /// then leaves camera and HUD alone).
    pub fn measuring(&self) -> bool {
        self.run.is_some() || !self.result.is_empty()
    }

    /// Each frame, after the game has set its camera.
    pub fn frame(
        &mut self,
        world: &mut World,
        frame: &Frame,
        grass: &mut Grass,
        scenery: &Scenery,
    ) -> EngineResult<()> {
        let now = Instant::now();
        let ms = self
            .last
            .replace(now)
            .map(|then| now.duration_since(then).as_secs_f32() * 1000.0);
        if let Some(ms) = ms {
            self.recent.push_back(ms);
            let mut total: f32 = self.recent.iter().sum();
            while total > 1000.0 && self.recent.len() > 1 {
                total -= self.recent.pop_front().unwrap_or(0.0);
            }
        }
        if self.shown && !self.recent.is_empty() {
            let average = self.recent.iter().sum::<f32>() / self.recent.len() as f32;
            let worst = self.recent.iter().copied().fold(0.0, f32::max);
            frame.ui.text(
                Anchor::TopRight,
                format!(
                    "{:.0} FPS   {average:.1} ms, worst {worst:.1} ms   {} grass clumps",
                    1000.0 / average.max(0.001),
                    grass.count()
                ),
            );
        }
        for line in &self.result {
            frame.ui.text(Anchor::TopLeft, line.clone());
        }
        let Some(run) = self.run.as_mut() else {
            if let Some(spot) = self.result.first().and(shots::measure_views().pop()) {
                world.look_at_up(spot.eye, spot.target, spot.up);
            }
            return Ok(());
        };
        let spot = &run.views[run.view];
        world.look_at_up(spot.eye, spot.target, spot.up);
        frame.ui.text(
            Anchor::Top,
            format!(
                "Measuring: {} of {}, {}",
                run.step * run.views.len() + run.view + 1,
                STEPS.len() * run.views.len(),
                spot.name
            ),
        );
        let Some(ms) = ms else { return Ok(()) };
        run.seconds += ms / 1000.0;
        // The very first view also waits out the start (shaders, uploads).
        let settle = run.settle
            + if run.step == 0 && run.view == 0 {
                3.0
            } else {
                0.0
            };
        if run.seconds > settle {
            run.frames += 1;
            run.sum += ms;
            run.worst = run.worst.max(ms);
        }
        if run.seconds < settle + run.hold || run.frames < 3 {
            return Ok(());
        }
        // This view is measured: on to the next view, or the next step.
        run.table[run.step].push((run.sum / run.frames as f32, run.worst));
        (run.seconds, run.frames, run.sum, run.worst) = (0.0, 0, 0.0, 0.0);
        run.view += 1;
        if run.view < run.views.len() {
            return Ok(());
        }
        run.view = 0;
        run.step += 1;
        run.table.push(Vec::new());
        match run.step {
            1 => grass.show(world, false)?,
            2 => scenery.show(world, &scenery.plants, false)?,
            3 => scenery.show(world, &scenery.built, false)?,
            4 => scenery.show(world, &scenery.cavern, false)?,
            _ => {
                // Done: everything back, and the result written down.
                grass.show(world, true)?;
                scenery.show(world, &scenery.plants, true)?;
                scenery.show(world, &scenery.built, true)?;
                scenery.show(world, &scenery.cavern, true)?;
                let lines = report(run, frame, grass.count());
                if let Err(e) = std::fs::write("measure.txt", lines.join("\n") + "\n") {
                    eprintln!("measure.txt: {e}");
                }
                for line in &lines {
                    println!("{line}");
                }
                world.report("measured views", run.views.len());
                self.result = lines;
                self.run = None;
            }
        }
        Ok(())
    }
}

/// The result in words: per view the frame rate with everything drawn
/// and what each part costs (the time saved by taking it away).
fn report(run: &Run, frame: &Frame, clumps: usize) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Grav frame times, Engine {}, window {} x {}, {clumps} grass clumps",
            engine::version(),
            frame.width,
            frame.height
        ),
        "Milliseconds per frame (average). 16.7 ms is 60 FPS.".to_string(),
    ];
    for (v, spot) in run.views.iter().enumerate() {
        let at = |step: usize| run.table[step][v];
        let (all, worst) = at(0);
        lines.push(String::new());
        lines.push(format!(
            "{}: {:.0} FPS ({all:.1} ms, worst frame {worst:.1} ms)",
            spot.name,
            1000.0 / all.max(0.001)
        ));
        let mut parts = Vec::new();
        for (step, name) in STEPS.iter().enumerate().skip(1) {
            parts.push(format!("{name} {:.1}", at(step - 1).0 - at(step).0));
        }
        parts.push(format!(
            "the rest (near rock, water, heart, hero, light) {:.1}",
            at(STEPS.len() - 1).0
        ));
        lines.push(format!("  {}", parts.join(", ")));
    }
    lines
}
