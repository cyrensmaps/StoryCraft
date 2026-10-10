//! Properties of the selected object, or of the slide, layer and course.

use egui::{ComboBox, DragValue, Grid, Ui};
use storycraft_model::{
    Advance, Color, Completion, HAlign, Object, ObjectKind, ObjectState, ShapeKind, Size, Stroke, TextContent, VAlign,
    WebDisplay, WebSource, state_names,
};

use crate::app::EditorApp;
use crate::widgets::{color_button, optional_color, seconds};

pub fn show(app: &mut EditorApp, ui: &mut Ui) {
    let focus_text = std::mem::take(&mut app.canvas.focus_text);
    let count = app.sel.objects.len();
    if count > 1 {
        ui.label(format!("{count} objects selected. Drag to move them together."));
        return;
    }
    let slide_size = app.project().slide_size;
    if let Some(object) = app.selected_mut() {
        object_properties(ui, object, slide_size, focus_text);
    } else {
        slide_properties(app, ui);
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(title).strong());
    ui.separator();
}

fn object_properties(ui: &mut Ui, o: &mut Object, slide: Size, focus_text: bool) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(o.kind.label()).weak());
        ui.add(egui::TextEdit::singleline(&mut o.name).hint_text("Name"));
    });
    if let ObjectKind::Shape { shape } = &mut o.kind {
        ComboBox::from_label("Shape").selected_text(shape.label()).show_ui(ui, |ui| {
            for s in ShapeKind::ALL {
                ui.selectable_value(shape, s, s.label());
            }
        });
    }
    if let ObjectKind::Web { source, display } = &mut o.kind {
        section(ui, "Web content");
        match source {
            WebSource::Url { url } => {
                ui.horizontal(|ui| {
                    ui.label("Address");
                    ui.add(egui::TextEdit::singleline(url).hint_text("https://").desired_width(f32::INFINITY));
                });
            }
            WebSource::Bundle { entry, .. } => {
                ui.label(format!("Local folder, opening {entry}"));
            }
        }
        ui.horizontal(|ui| {
            ui.label("Show");
            ui.selectable_value(display, WebDisplay::OnSlide, "On the slide");
            ui.selectable_value(display, WebDisplay::NewWindow, "In a new window");
        });
        ui.label(
            egui::RichText::new("Web pages show in published courses and in Preview > Preview in browser.")
                .weak()
                .small(),
        );
    }

    section(ui, "Position and size");
    Grid::new("geometry").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
        let max = slide.width.max(slide.height) * 4.0;
        ui.label("X");
        ui.add(DragValue::new(&mut o.frame.x).range(-max..=max));
        ui.label("Y");
        ui.add(DragValue::new(&mut o.frame.y).range(-max..=max));
        ui.end_row();
        ui.label("Width");
        ui.add(DragValue::new(&mut o.frame.width).range(1.0..=max));
        ui.label("Height");
        ui.add(DragValue::new(&mut o.frame.height).range(1.0..=max));
        ui.end_row();
        ui.label("Rotation");
        ui.add(DragValue::new(&mut o.rotation).range(-360.0..=360.0).suffix("°"));
        ui.end_row();
    });

    section(ui, "Style");
    if !matches!(o.kind, ObjectKind::Image { .. }) {
        optional_color(ui, "Fill", &mut o.style.fill, Color::rgb(0x5B, 0x9B, 0xD5));
    }
    ui.horizontal(|ui| {
        let mut on = o.style.stroke.is_some();
        if ui.checkbox(&mut on, "Outline").changed() {
            o.style.stroke = on.then_some(Stroke { color: Color::BLACK, width: 2.0 });
        }
        if let Some(stroke) = &mut o.style.stroke {
            color_button(ui, &mut stroke.color);
            ui.add(DragValue::new(&mut stroke.width).range(0.0..=50.0).suffix(" px"));
        }
    });
    if matches!(o.kind, ObjectKind::Button | ObjectKind::Text | ObjectKind::Shape { shape: ShapeKind::Rectangle }) {
        ui.horizontal(|ui| {
            ui.label("Corner radius");
            ui.add(DragValue::new(&mut o.style.corner_radius).range(0.0..=500.0));
        });
    }
    ui.horizontal(|ui| {
        ui.label("Opacity");
        ui.add(egui::Slider::new(&mut o.style.opacity, 0.0..=1.0));
    });

    section(ui, "Text");
    let mut has_text = o.text.is_some();
    if !matches!(o.kind, ObjectKind::Text) && ui.checkbox(&mut has_text, "Show text").changed() {
        o.text = has_text.then(|| TextContent::new(""));
    }
    if let Some(t) = &mut o.text {
        text_properties(ui, t, focus_text);
    }

    section(ui, "Timing");
    ui.horizontal(|ui| {
        ui.label("Appears at");
        seconds(ui, &mut o.timing.start_ms);
    });
    ui.horizontal(|ui| {
        let mut until_end = o.timing.duration_ms.is_none();
        if ui.checkbox(&mut until_end, "Show until the end").changed() {
            o.timing.duration_ms = if until_end { None } else { Some(3000) };
        }
        if let Some(d) = &mut o.timing.duration_ms {
            ui.label("for");
            seconds(ui, d);
        }
    });
    ui.checkbox(&mut o.initially_hidden, "Initially hidden")
        .on_hover_text("Starts in the Hidden state. Use a Change state trigger to show it.");

    section(ui, "Accessibility");
    ui.add(egui::TextEdit::singleline(&mut o.alt_text).hint_text("Alternative text (leave empty if decorative)"));

    section(ui, "States");
    states_editor(ui, o);
}

fn text_properties(ui: &mut Ui, t: &mut TextContent, focus: bool) {
    let r = ui.add(egui::TextEdit::multiline(&mut t.text).desired_rows(3).desired_width(f32::INFINITY));
    if focus {
        r.request_focus();
    }
    ui.label(egui::RichText::new("Insert a variable with %Name%").weak().small());
    ui.horizontal(|ui| {
        ui.add(DragValue::new(&mut t.font_size).range(4.0..=400.0).suffix(" px"));
        color_button(ui, &mut t.color);
        ui.toggle_value(&mut t.bold, egui::RichText::new("B").strong());
        ui.toggle_value(&mut t.italic, egui::RichText::new("I").italics());
    });
    ui.horizontal(|ui| {
        ui.selectable_value(&mut t.align, HAlign::Left, "Left");
        ui.selectable_value(&mut t.align, HAlign::Center, "Center");
        ui.selectable_value(&mut t.align, HAlign::Right, "Right");
        ui.separator();
        ui.selectable_value(&mut t.valign, VAlign::Top, "Top");
        ui.selectable_value(&mut t.valign, VAlign::Middle, "Middle");
        ui.selectable_value(&mut t.valign, VAlign::Bottom, "Bottom");
    });
    ui.horizontal(|ui| {
        ui.label("Font");
        ui.add(egui::TextEdit::singleline(&mut t.font_family).desired_width(160.0))
            .on_hover_text("CSS font family used in published courses, e.g. Georgia, serif");
    });
}

fn states_editor(ui: &mut Ui, o: &mut Object) {
    ui.label(egui::RichText::new("Normal and Hidden are always available.").weak().small());
    let mut remove = None;
    let has_text = o.text.is_some();
    let is_image = matches!(o.kind, ObjectKind::Image { .. });
    for (i, state) in o.states.iter_mut().enumerate() {
        egui::CollapsingHeader::new(&state.name).id_salt(("state", i)).show(ui, |ui| {
            let ov = &mut state.overrides;
            if !is_image {
                optional_color(ui, "Fill", &mut ov.fill, Color::rgb(0x3A, 0x82, 0xCF));
            }
            optional_color(ui, "Outline colour", &mut ov.stroke_color, Color::BLACK);
            if has_text {
                optional_color(ui, "Text colour", &mut ov.text_color, Color::BLACK);
                ui.horizontal(|ui| {
                    let mut on = ov.text.is_some();
                    if ui.checkbox(&mut on, "Different text").changed() {
                        ov.text = on.then(String::new);
                    }
                    if let Some(t) = &mut ov.text {
                        ui.text_edit_singleline(t);
                    }
                });
            }
            ui.horizontal(|ui| {
                let mut on = ov.opacity.is_some();
                if ui.checkbox(&mut on, "Opacity").changed() {
                    ov.opacity = on.then_some(1.0);
                }
                if let Some(op) = &mut ov.opacity {
                    ui.add(egui::Slider::new(op, 0.0..=1.0));
                }
            });
            if ui.small_button("Delete state").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        o.states.remove(i);
    }

    ui.horizontal(|ui| {
        let missing: Vec<&str> = state_names::BUILT_IN
            .iter()
            .copied()
            .filter(|n| *n != state_names::NORMAL && *n != state_names::HIDDEN && o.state(n).is_none())
            .collect();
        ComboBox::from_id_salt("add-state").selected_text("+ Add state").show_ui(ui, |ui| {
            for name in missing {
                if ui.button(name).clicked() {
                    o.states.push(ObjectState::new(name));
                }
            }
        });
        let id = ui.make_persistent_id("custom-state");
        let mut name: String = ui.data_mut(|d| d.get_temp(id).unwrap_or_default());
        ui.add(egui::TextEdit::singleline(&mut name).hint_text("Custom state").desired_width(110.0));
        let valid = !name.trim().is_empty() && !o.has_state(name.trim());
        if ui.add_enabled(valid, egui::Button::new("Add")).clicked() {
            o.states.push(ObjectState::new(name.trim()));
            name.clear();
        }
        ui.data_mut(|d| d.insert_temp(id, name));
    });
}

fn slide_properties(app: &mut EditorApp, ui: &mut Ui) {
    let is_base = app.is_base_layer();
    let slide_id = app.slide().id.clone();
    let slide = app.slide_mut();
    ui.heading("Slide");
    ui.horizontal(|ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut slide.name);
    });
    ui.horizontal(|ui| {
        ui.label("Background");
        color_button(ui, &mut slide.background);
    });
    ui.horizontal(|ui| {
        ui.label("Advance");
        ComboBox::from_id_salt("advance")
            .selected_text(match slide.advance {
                Advance::Automatically => "Automatically",
                Advance::ByUser => "By user",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut slide.advance, Advance::Automatically, "Automatically");
                ui.selectable_value(&mut slide.advance, Advance::ByUser, "By user");
            });
    });
    ui.label("Notes");
    ui.add(egui::TextEdit::multiline(&mut slide.notes).desired_rows(3).desired_width(f32::INFINITY));

    if !is_base {
        let layer = app.layer_mut();
        ui.add_space(8.0);
        ui.heading("Layer");
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut layer.name);
        });
        ui.checkbox(&mut layer.hide_other_layers, "Hide other layers when shown");
        ui.checkbox(&mut layer.pause_base_timeline, "Pause the base layer timeline while shown");
        ui.checkbox(&mut layer.prevent_base_clicks, "Prevent clicking on the base layer while shown");
        ui.checkbox(&mut layer.hide_when_timeline_ends, "Hide this layer when its timeline ends");
    }

    ui.add_space(8.0);
    egui::CollapsingHeader::new(egui::RichText::new("Course settings").heading()).default_open(false).show(ui, |ui| {
        course_settings(app, ui, &slide_id);
    });
}

fn course_settings(app: &mut EditorApp, ui: &mut Ui, current_slide: &storycraft_model::Id) {
    let p = &mut app.doc.project;
    Grid::new("course").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Title");
        ui.text_edit_singleline(&mut p.title);
        ui.end_row();
        ui.label("Author");
        ui.text_edit_singleline(&mut p.author);
        ui.end_row();
        ui.label("Slide size");
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut p.slide_size.width).range(100.0..=4000.0));
            ui.label("×");
            ui.add(DragValue::new(&mut p.slide_size.height).range(100.0..=4000.0));
            ComboBox::from_id_salt("size-preset").selected_text("Preset").show_ui(ui, |ui| {
                if ui.button("16:9 (1280 × 720)").clicked() {
                    p.slide_size = Size { width: 1280.0, height: 720.0 };
                }
                if ui.button("4:3 (720 × 540)").clicked() {
                    p.slide_size = Size { width: 720.0, height: 540.0 };
                }
            });
        });
        ui.end_row();
        ui.label("Player buttons");
        ui.checkbox(&mut p.player.show_nav_buttons, "Show Previous/Next");
        ui.end_row();
        ui.label("Backdrop");
        color_button(ui, &mut p.player.backdrop);
        ui.end_row();
        ui.label("Start slide");
        let is_start = p.start_slide_id() == Some(current_slide);
        if is_start {
            ui.label("This slide");
        } else if ui.button("Start on this slide").clicked() {
            p.start_slide = Some(current_slide.clone());
        }
        ui.end_row();
    });

    ui.add_space(6.0);
    ui.label(egui::RichText::new("LMS reporting").strong());
    Grid::new("lms").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Course ID");
        ui.add(egui::TextEdit::singleline(&mut p.lms.course_id).hint_text("(automatic)"));
        ui.end_row();
        ui.label("Complete when");
        let label = match p.lms.completion {
            Completion::AllSlidesViewed => "All slides viewed".to_owned(),
            Completion::SlidesViewed { count } => format!("{count} slides viewed"),
            Completion::OnTrigger => "A trigger completes it".to_owned(),
        };
        ComboBox::from_id_salt("completion").selected_text(label).show_ui(ui, |ui| {
            ui.selectable_value(&mut p.lms.completion, Completion::AllSlidesViewed, "All slides viewed");
            if ui
                .selectable_label(matches!(p.lms.completion, Completion::SlidesViewed { .. }), "Some slides viewed")
                .clicked()
            {
                p.lms.completion = Completion::SlidesViewed { count: 1 };
            }
            ui.selectable_value(&mut p.lms.completion, Completion::OnTrigger, "A trigger completes it");
        });
        ui.end_row();
        if let Completion::SlidesViewed { count } = &mut p.lms.completion {
            ui.label("Slides to view");
            ui.add(DragValue::new(count).range(1..=10_000));
            ui.end_row();
        }
    });
}
