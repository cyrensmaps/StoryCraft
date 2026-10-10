//! Small reusable editing widgets.

use egui::{ComboBox, DragValue, Response, Ui};
use storycraft_model::{
    Color, Id, LayerTarget, Object, Operand, Project, Slide, SlideTarget, Value, ValueKind, state_names,
};

use crate::draw::{color32, from_color32};

pub fn color_button(ui: &mut Ui, color: &mut Color) -> Response {
    let mut c = color32(*color).to_srgba_unmultiplied();
    let response = ui.color_edit_button_srgba_unmultiplied(&mut c);
    *color = from_color32(egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]));
    response
}

/// A checkbox that turns an optional colour on or off, plus its picker.
pub fn optional_color(ui: &mut Ui, label: &str, color: &mut Option<Color>, default: Color) {
    ui.horizontal(|ui| {
        let mut on = color.is_some();
        if ui.checkbox(&mut on, label).changed() {
            *color = on.then_some(default);
        }
        if let Some(c) = color {
            color_button(ui, c);
        }
    });
}

/// Edit a millisecond value as seconds.
pub fn seconds(ui: &mut Ui, ms: &mut u32) -> Response {
    let mut s = *ms as f64 / 1000.0;
    let r = ui.add(DragValue::new(&mut s).speed(0.05).range(0.0..=3600.0).suffix(" s").max_decimals(2));
    *ms = (s * 1000.0).round() as u32;
    r
}

fn object_label(o: &Object) -> String {
    if o.name.is_empty() { format!("({})", o.kind.label()) } else { o.name.clone() }
}

/// Pick any object on `slide` (all layers).
pub fn object_picker(ui: &mut Ui, salt: impl std::hash::Hash + std::fmt::Debug, slide: &Slide, selected: &mut Id) {
    let current = slide.find_object(selected).map_or_else(|| "(choose)".to_owned(), |(_, o)| object_label(o));
    ComboBox::from_id_salt(salt).selected_text(current).width(180.0).show_ui(ui, |ui| {
        for layer in slide.layers() {
            ui.label(egui::RichText::new(&layer.name).weak().small());
            for o in &layer.objects {
                ui.selectable_value(selected, o.id.clone(), object_label(o));
            }
        }
    });
}

pub fn slide_target_picker(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    project: &Project,
    target: &mut SlideTarget,
) {
    let label = match target {
        SlideTarget::Next => "Next slide".to_owned(),
        SlideTarget::Previous => "Previous slide".to_owned(),
        SlideTarget::Slide(id) => project.slide(id).map_or("(missing slide)".into(), |s| s.name.clone()),
    };
    ComboBox::from_id_salt(salt).selected_text(label).width(180.0).show_ui(ui, |ui| {
        ui.selectable_value(target, SlideTarget::Next, "Next slide");
        ui.selectable_value(target, SlideTarget::Previous, "Previous slide");
        ui.separator();
        for (loc, slide) in project.slides() {
            ui.selectable_value(
                target,
                SlideTarget::Slide(slide.id.clone()),
                format!("{}.{} {}", loc.scene + 1, loc.slide + 1, slide.name),
            );
        }
    });
}

/// Pick one of the slide's non-base layers.
pub fn layer_picker(ui: &mut Ui, salt: impl std::hash::Hash + std::fmt::Debug, slide: &Slide, id: &mut Id) {
    let label = slide.layer(id).map_or("(choose)".to_owned(), |l| l.name.clone());
    ComboBox::from_id_salt(salt).selected_text(label).width(180.0).show_ui(ui, |ui| {
        for layer in &slide.layers {
            ui.selectable_value(id, layer.id.clone(), &layer.name);
        }
    });
}

pub fn layer_target_picker(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    slide: &Slide,
    target: &mut LayerTarget,
) {
    let label = match target {
        LayerTarget::This => "This layer".to_owned(),
        LayerTarget::Layer(id) => slide.layer(id).map_or("(missing layer)".into(), |l| l.name.clone()),
    };
    ComboBox::from_id_salt(salt).selected_text(label).width(180.0).show_ui(ui, |ui| {
        ui.selectable_value(target, LayerTarget::This, "This layer");
        for layer in slide.layers() {
            ui.selectable_value(target, LayerTarget::Layer(layer.id.clone()), &layer.name);
        }
    });
}

pub fn variable_picker(ui: &mut Ui, salt: impl std::hash::Hash + std::fmt::Debug, project: &Project, id: &mut Id) {
    let label = project.variable(id).map_or("(choose)".to_owned(), |v| v.name.clone());
    ComboBox::from_id_salt(salt).selected_text(label).width(160.0).show_ui(ui, |ui| {
        for v in &project.variables {
            ui.selectable_value(id, v.id.clone(), &v.name);
        }
    });
}

/// Pick a state of `object`: built-in Normal/Hidden plus its declared ones.
pub fn state_picker(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    object: Option<&Object>,
    state: &mut String,
) {
    ComboBox::from_id_salt(salt).selected_text(state.as_str()).width(140.0).show_ui(ui, |ui| {
        for name in [state_names::NORMAL, state_names::HIDDEN] {
            ui.selectable_value(state, name.to_owned(), name);
        }
        if let Some(o) = object {
            for s in &o.states {
                ui.selectable_value(state, s.name.clone(), &s.name);
            }
        }
    });
}

/// Edit a literal value, keeping it of type `kind`.
pub fn value_editor(ui: &mut Ui, value: &mut Value, kind: ValueKind) {
    if value.kind() != kind {
        *value = value.coerce(kind);
    }
    match value {
        Value::Text(t) => {
            ui.add(egui::TextEdit::singleline(t).desired_width(140.0));
        }
        Value::Number(n) => {
            ui.add(DragValue::new(n).speed(1.0));
        }
        Value::Bool(b) => {
            ComboBox::from_id_salt(ui.next_auto_id()).selected_text(if *b { "True" } else { "False" }).show_ui(
                ui,
                |ui| {
                    ui.selectable_value(b, true, "True");
                    ui.selectable_value(b, false, "False");
                },
            );
        }
    }
}

/// Edit an operand: a literal of `kind`, or another variable.
pub fn operand_editor(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug + Copy,
    project: &Project,
    operand: &mut Operand,
    kind: ValueKind,
) {
    let is_var = matches!(operand, Operand::Variable(_));
    ComboBox::from_id_salt((salt, "operand-kind"))
        .selected_text(if is_var { "Variable" } else { "Value" })
        .width(80.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(!is_var, "Value").clicked() && is_var {
                *operand = Operand::Literal(kind.default_value());
            }
            if ui.selectable_label(is_var, "Variable").clicked() && !is_var {
                let first = project.variables.first().map(|v| v.id.clone()).unwrap_or_default();
                *operand = Operand::Variable(first);
            }
        });
    match operand {
        Operand::Literal(v) => value_editor(ui, v, kind),
        Operand::Variable(id) => variable_picker(ui, (salt, "operand-var"), project, id),
    }
}
