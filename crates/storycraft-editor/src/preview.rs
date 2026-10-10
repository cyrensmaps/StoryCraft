//! In-app course preview, driven by the same engine as published courses.

use std::collections::BTreeMap;

use egui::{CursorIcon, Key, Sense, Ui};
use storycraft_engine::{Effect, Engine, Snapshot};
use storycraft_model::{Id, Project};

use crate::draw::{SlideView, draw_frame};
use crate::textures::Textures;

pub struct Preview {
    project: Project,
    engine: Engine,
    pressed_on_stage: bool,
    note: Option<String>,
}

impl Preview {
    pub fn new(project: &Project, from: Option<&Id>) -> Result<Self, String> {
        let engine = start_engine(project, from)?;
        Ok(Preview { project: project.clone(), engine, pressed_on_stage: false, note: None })
    }

    /// Show the preview. Returns true when the author closes it.
    pub fn ui(&mut self, ui: &mut Ui, textures: &mut Textures, media: &BTreeMap<Id, Vec<u8>>) -> bool {
        let ctx = ui.ctx().clone();
        let mut close = false;

        ui.horizontal(|ui| {
            if ui.button("■ Close preview").clicked() {
                close = true;
            }
            if ui.button("⟲ Restart").clicked() {
                if let Ok(engine) = start_engine(&self.project, None) {
                    self.engine = engine;
                    self.note = None;
                }
            }
            ui.separator();
            if ui.add_enabled(self.engine.can_go_back(), egui::Button::new("◀ Previous")).clicked() {
                self.engine.previous_slide();
            }
            let (n, total) = self.engine.position();
            ui.label(format!("Slide {n} of {total}"));
            if ui.add_enabled(self.engine.can_go_forward(), egui::Button::new("Next ▶")).clicked() {
                self.engine.next_slide();
            }
            ui.separator();
            if self.engine.is_completed() {
                ui.colored_label(egui::Color32::from_rgb(0x2E, 0x9E, 0x4F), "✔ Complete");
            }
            if let Some(note) = &self.note {
                ui.label(note);
            }
        });

        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        painter.rect_filled(response.rect, 0.0, egui::Color32::from_gray(34));
        let view = SlideView::fit(response.rect, self.project.slide_size, 16.0);
        let stage = view.slide_rect(self.project.slide_size);

        // Pointer input in slide coordinates.
        let hover = response.hover_pos().filter(|p| stage.contains(*p));
        self.engine.pointer_move(hover.map(|p| view.to_slide(p)));
        if let Some(p) = hover {
            if self.engine.hit_test(view.to_slide(p)).is_some() {
                ctx.set_cursor_icon(CursorIcon::PointingHand);
            }
        }
        let (pressed, released, latest) =
            ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.latest_pos()));
        if pressed {
            if let Some(p) = hover {
                self.pressed_on_stage = true;
                self.engine.pointer_down(view.to_slide(p));
            }
        }
        if released && self.pressed_on_stage {
            self.pressed_on_stage = false;
            if let Some(p) = latest {
                self.engine.pointer_up(view.to_slide(p));
            }
        }

        // Keys, unless a text field elsewhere has focus.
        if !ctx.egui_wants_keyboard_input() {
            let keys: Vec<Key> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(*key),
                        _ => None,
                    })
                    .collect()
            });
            for key in keys {
                self.engine.key_pressed(&dom_key(key));
            }
        }

        let dt = ctx.input(|i| i.stable_dt).min(0.25) as f64 * 1000.0;
        self.engine.tick(dt);

        for effect in self.engine.take_effects() {
            match effect {
                Effect::OpenUrl(url) => {
                    if url.starts_with("http://") || url.starts_with("https://") {
                        crate::app::open_in_system(std::path::Path::new(&url));
                    }
                }
                Effect::Exit => {
                    self.note = Some("The course asked to exit.".into());
                }
                Effect::Completed => {
                    self.note = Some("Completion would be reported to the LMS now.".into());
                }
                Effect::SlideChanged(_) => {}
                Effect::Notice(message) => self.note = Some(message),
                Effect::QuizSubmitted(r) => {
                    let verdict = if r.passed { "passed" } else { "failed" };
                    self.note = Some(format!("Quiz submitted: {}% ({verdict})", r.percent));
                }
            }
        }

        let frame = self.engine.frame();
        draw_frame(&painter, view, &frame, &mut |id| textures.get(&ctx, id, media), &|_| 1.0);
        ctx.request_repaint();
        close
    }
}

fn start_engine(project: &Project, from: Option<&Id>) -> Result<Engine, String> {
    let mut engine = Engine::new(project.clone()).map_err(|e| e.to_string())?;
    if let Some(id) = from {
        engine.restore(&Snapshot { slide: Some(id.clone()), ..Default::default() });
    }
    engine.start();
    Ok(engine)
}

/// Translate an egui key into the DOM `KeyboardEvent.key` name that
/// published courses use, so key triggers behave the same in both.
fn dom_key(key: Key) -> String {
    let named = match key {
        Key::ArrowDown => "ArrowDown",
        Key::ArrowLeft => "ArrowLeft",
        Key::ArrowRight => "ArrowRight",
        Key::ArrowUp => "ArrowUp",
        Key::Space => " ",
        Key::Comma => ",",
        Key::Minus => "-",
        Key::Period => ".",
        Key::Plus => "+",
        Key::Equals => "=",
        Key::Semicolon => ";",
        Key::Colon => ":",
        Key::Slash => "/",
        Key::Backslash => "\\",
        Key::Questionmark => "?",
        Key::Exclamationmark => "!",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Backtick => "`",
        other => {
            let name = other.name();
            return if name.chars().count() == 1 { name.to_lowercase() } else { name.to_owned() };
        }
    };
    named.to_owned()
}
