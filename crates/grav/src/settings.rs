//! The player's settings: a gear button in the top-left corner opens a
//! window to change them; they are kept in `grav-settings.json` in the
//! working folder and applied at start.

use engine::egui::{self, Align2, Color32, Pos2, Sense, Shape as EguiShape, Stroke};
use engine::prelude::*;
use serde::{Deserialize, Serialize};

const FILE: &str = "grav-settings.json";

/// What the player can set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Graphics quality: low, medium, high or ultra.
    pub quality: String,
    pub vsync: bool,
    /// Mouse look speed (degrees per pixel of mouse travel).
    pub look_speed: f32,
    /// Moving the mouse up looks down.
    pub invert_look: bool,
    /// The pull readout in the top-right corner.
    pub show_pull: bool,
    /// The line of controls at the bottom.
    pub show_controls: bool,
    /// What "1 g" means, m/s² (grav_core::one_g): to find what plays well.
    pub one_g: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            quality: Quality::High.name().into(),
            vsync: false,
            look_speed: 0.15,
            invert_look: false,
            show_pull: true,
            show_controls: true,
            one_g: grav_core::DEFAULT_ONE_G,
        }
    }
}

/// The settings and their window.
pub struct SettingsUi {
    pub prefs: Prefs,
    pub open: bool,
}

impl SettingsUi {
    /// Read the saved settings (defaults when there are none) and apply
    /// them.
    pub fn load(world: &mut World) -> Self {
        let prefs = std::fs::read_to_string(FILE)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        // GRAV_SHOW_SETTINGS=1 opens the window at start (for screenshots).
        let open = std::env::var("GRAV_SHOW_SETTINGS").is_ok();
        let ui = Self { prefs, open };
        ui.apply(world);
        ui
    }

    fn apply(&self, world: &mut World) {
        world.set_quality(Quality::from_name(&self.prefs.quality).unwrap_or_default());
        world.set_vsync(self.prefs.vsync);
        grav_core::set_one_g(self.prefs.one_g);
    }

    fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(&self.prefs) {
            if let Err(e) = std::fs::write(FILE, text) {
                eprintln!("{FILE}: {e}");
            }
        }
    }

    /// The gear button, and the window while it is open (Escape or a
    /// click beside it closes it, without quitting the game).
    pub fn show(&mut self, frame_ui: &UiFrame, world: &mut World) {
        let ctx = frame_ui.ctx();
        egui::Area::new(egui::Id::new("settings button"))
            .anchor(Align2::LEFT_TOP, [12.0, 12.0])
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), Sense::click());
                let hot = resp.hovered() || self.open;
                let painter = ui.painter();
                painter.rect_filled(
                    rect,
                    10.0,
                    Color32::from_rgba_unmultiplied(18, 16, 14, if hot { 235 } else { 190 }),
                );
                let ink = if hot {
                    Color32::from_rgb(255, 214, 150)
                } else {
                    Color32::from_gray(215)
                };
                gear(painter, rect.center(), 13.0, ink);
                if resp.on_hover_text("Settings").clicked() {
                    self.open = !self.open;
                }
            });
        if !self.open {
            return;
        }
        let before = self.prefs.clone();
        let p = &mut self.prefs;
        let mut open = true;
        frame_ui.modal("Settings", &mut open, |panel, _| {
            let ui = panel.ui();
            ui.set_min_width(300.0);
            let heading = |ui: &mut egui::Ui, text: &str| {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(text)
                        .size(13.0)
                        .color(Color32::from_rgb(220, 180, 120)),
                );
            };
            heading(ui, "Graphics");
            egui::Grid::new("graphics")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Quality");
                    egui::ComboBox::from_id_salt("quality")
                        .selected_text(capital(&p.quality))
                        .show_ui(ui, |ui| {
                            for q in Quality::ALL {
                                ui.selectable_value(
                                    &mut p.quality,
                                    q.name().to_string(),
                                    capital(q.name()),
                                );
                            }
                        });
                    ui.end_row();
                    ui.label("Vertical sync");
                    ui.checkbox(&mut p.vsync, "");
                    ui.end_row();
                });
            heading(ui, "World");
            egui::Grid::new("world")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label("1 g is")
                        .on_hover_text("Every pull in g scales with this. Earth's is 9.81.");
                    ui.add(
                        egui::Slider::new(&mut p.one_g, 1.0..=12.0)
                            .fixed_decimals(2)
                            .suffix(" m/s²"),
                    );
                    ui.end_row();
                    ui.label("");
                    ui.label(
                        egui::RichText::new(format!(
                            "{:.0}% of Earth's",
                            p.one_g / grav_core::EARTH_G * 100.0
                        ))
                        .size(12.0)
                        .color(Color32::from_gray(150)),
                    );
                    ui.end_row();
                });
            heading(ui, "Controls");
            egui::Grid::new("controls")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Mouse look speed");
                    ui.add(egui::Slider::new(&mut p.look_speed, 0.03..=0.6).fixed_decimals(2));
                    ui.end_row();
                    ui.label("Invert mouse look");
                    ui.checkbox(&mut p.invert_look, "");
                    ui.end_row();
                });
            heading(ui, "Screen");
            egui::Grid::new("screen")
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Pull readout");
                    ui.checkbox(&mut p.show_pull, "");
                    ui.end_row();
                    ui.label("Controls line");
                    ui.checkbox(&mut p.show_controls, "");
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Defaults").clicked() {
                    *p = Prefs::default();
                }
                ui.label(
                    egui::RichText::new("Esc closes")
                        .size(12.0)
                        .color(Color32::from_gray(150)),
                );
            });
        });
        self.open = open;
        if self.prefs != before {
            self.apply(world);
            self.save();
        }
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// A cog: eight teeth around a ring with a hole.
fn gear(painter: &egui::Painter, c: Pos2, r: f32, ink: Color32) {
    let teeth = 8;
    let mut outline = Vec::new();
    for i in 0..teeth * 4 {
        let a = i as f32 / (teeth * 4) as f32 * std::f32::consts::TAU;
        let tip = matches!(i % 4, 1 | 2);
        let radius = if tip { r } else { r * 0.74 };
        outline.push(c + egui::vec2(a.cos(), a.sin()) * radius);
    }
    painter.add(EguiShape::closed_line(outline, Stroke::new(2.2_f32, ink)));
    painter.circle_stroke(c, r * 0.32, Stroke::new(2.2_f32, ink));
}
