//! Course-wide variables.

use egui::{ComboBox, Grid, Ui};
use storycraft_model::{Id, Project, Value, ValueKind, Variable};

use crate::app::EditorApp;
use crate::widgets::value_editor;

pub fn show(app: &mut EditorApp, ui: &mut Ui) {
    ui.label("Variables hold values for the whole course. Show one in text with %Name%.");
    ui.add_space(4.0);
    if ui.button("+ New variable").clicked() {
        let n = (1..).find(|n| app.project().variable_by_name(&format!("Variable{n}")).is_none()).unwrap();
        app.doc.project.variables.push(Variable::new(format!("Variable{n}"), Value::Number(0.0)));
    }
    ui.add_space(6.0);

    let uses: Vec<usize> = app.project().variables.iter().map(|v| count_uses(app.project(), &v.id)).collect();
    let names: Vec<String> = app.project().variables.iter().map(|v| v.name.clone()).collect();
    let mut remove = None;
    Grid::new("variables").num_columns(5).striped(true).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("Name").strong());
        ui.label(egui::RichText::new("Type").strong());
        ui.label(egui::RichText::new("Initial value").strong());
        ui.label(egui::RichText::new("Used").strong());
        ui.end_row();
        for (i, var) in app.doc.project.variables.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                let duplicate = names.iter().filter(|n| **n == var.name).count() > 1;
                let invalid =
                    var.name.is_empty() || !var.name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.');
                let edit = ui.add(egui::TextEdit::singleline(&mut var.name).desired_width(110.0));
                if duplicate || invalid {
                    edit.on_hover_text(if duplicate {
                        "Another variable has this name"
                    } else {
                        "Use letters, digits, _ and . only"
                    });
                    ui.colored_label(ui.visuals().error_fg_color, "⚠");
                }
            });
            let mut kind = var.value.kind();
            ComboBox::from_id_salt(("var-kind", i)).selected_text(kind.label()).width(90.0).show_ui(ui, |ui| {
                for k in ValueKind::ALL {
                    ui.selectable_value(&mut kind, k, k.label());
                }
            });
            ui.push_id(("var-value", i), |ui| value_editor(ui, &mut var.value, kind));
            ui.label(uses[i].to_string()).on_hover_text("Number of triggers and conditions using this variable");
            if ui.small_button("✖").on_hover_text("Delete variable").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        app.doc.project.variables.remove(i);
    }
}

/// How many triggers refer to the variable (in events, actions or conditions).
fn count_uses(project: &Project, id: &Id) -> usize {
    let needle = format!("\"{id}\"");
    project
        .slides()
        .flat_map(|(_, s)| s.layers().flat_map(|l| l.triggers.iter()).collect::<Vec<_>>())
        .filter(|t| serde_json::to_string(t).is_ok_and(|json| json.contains(&needle)))
        .count()
}
