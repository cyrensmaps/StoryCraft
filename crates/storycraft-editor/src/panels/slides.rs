//! The scene and slide list.

use egui::Ui;
use storycraft_model::SlideLocation;

use crate::app::EditorApp;

enum SlideAction {
    Select(SlideLocation),
    Duplicate(SlideLocation),
    Delete(SlideLocation),
    Move(SlideLocation, bool),
    SetStart(SlideLocation),
    AddSlideTo(usize),
    DeleteScene(usize),
}

pub fn show(app: &mut EditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.heading("Slides");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("+ Scene").clicked() {
                app.add_scene();
            }
        });
    });
    ui.separator();

    let mut action = None;
    let scene_count = app.project().scenes.len();
    let start = app.project().start_slide_id().cloned();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for si in 0..scene_count {
            let scene_id = app.project().scenes[si].id.clone();
            let mut name = app.project().scenes[si].name.clone();
            let header = egui::CollapsingHeader::new(format!("{}. {}", si + 1, name))
                .id_salt(&scene_id)
                .default_open(true)
                .show(ui, |ui| {
                    let slides: Vec<_> =
                        app.project().scenes[si].slides.iter().map(|s| (s.id.clone(), s.name.clone())).collect();
                    for (li, (id, slide_name)) in slides.iter().enumerate() {
                        let loc = SlideLocation { scene: si, slide: li };
                        let flag = if start.as_ref() == Some(id) { "  ⚑" } else { "" };
                        let r = ui.selectable_label(
                            loc == app.sel.slide,
                            format!("{}.{}  {slide_name}{flag}", si + 1, li + 1),
                        );
                        if r.clicked() {
                            action = Some(SlideAction::Select(loc));
                        }
                        r.context_menu(|ui| {
                            if ui.button("Duplicate").clicked() {
                                action = Some(SlideAction::Duplicate(loc));
                            }
                            if ui.button("Delete").clicked() {
                                action = Some(SlideAction::Delete(loc));
                            }
                            ui.separator();
                            if ui.add_enabled(li > 0, egui::Button::new("Move up")).clicked() {
                                action = Some(SlideAction::Move(loc, true));
                            }
                            if ui.add_enabled(li + 1 < slides.len(), egui::Button::new("Move down")).clicked() {
                                action = Some(SlideAction::Move(loc, false));
                            }
                            ui.separator();
                            if ui.button("Start the course here").clicked() {
                                action = Some(SlideAction::SetStart(loc));
                            }
                        });
                    }
                    if ui.small_button("+ Slide").clicked() {
                        action = Some(SlideAction::AddSlideTo(si));
                    }
                });
            header.header_response.context_menu(|ui| {
                ui.label("Scene name");
                if ui.text_edit_singleline(&mut name).changed() {
                    app.doc.project.scenes[si].name = name.clone();
                }
                ui.separator();
                if ui.add_enabled(scene_count > 1, egui::Button::new("Delete scene")).clicked() {
                    action = Some(SlideAction::DeleteScene(si));
                }
            });
        }
    });

    match action {
        Some(SlideAction::Select(loc)) => app.select_slide(loc),
        Some(SlideAction::Duplicate(loc)) => app.duplicate_slide(loc),
        Some(SlideAction::Delete(loc)) => app.delete_slide(loc),
        Some(SlideAction::Move(loc, up)) => app.move_slide(loc, up),
        Some(SlideAction::SetStart(loc)) => {
            let id = app.project().slide_at(loc).map(|s| s.id.clone());
            app.doc.project.start_slide = id;
        }
        Some(SlideAction::AddSlideTo(scene)) => {
            let last = app.project().scenes[scene].slides.len().saturating_sub(1);
            app.select_slide(SlideLocation { scene, slide: last });
            app.add_slide();
        }
        Some(SlideAction::DeleteScene(scene)) => {
            app.doc.project.scenes.remove(scene);
            app.select_slide(SlideLocation { scene: 0, slide: 0 });
        }
        None => {}
    }
}
