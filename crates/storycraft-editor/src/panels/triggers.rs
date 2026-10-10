//! The trigger list for the layer being edited.

use std::mem::discriminant;

use egui::{ComboBox, Ui};
use storycraft_model::{
    Action, Comparison, Condition, ConditionClause, Event, Id, Join, LayerTarget, Operand, Project, Slide, SlideTarget,
    Trigger, Value, ValueKind, VarOp, state_names,
};

use crate::app::EditorApp;
use crate::widgets::{
    layer_picker, layer_target_picker, object_picker, operand_editor, slide_target_picker, state_picker,
    variable_picker,
};

pub fn show(app: &mut EditorApp, ui: &mut Ui) {
    let layer_name = app.layer().name.clone();
    ui.label(format!("Triggers on {layer_name}. They run while this layer is visible."));
    ui.add_space(4.0);

    // Edit a copy so the project can be read by the pickers at the same time.
    let mut triggers = app.layer().triggers.clone();
    let project = app.project();
    let slide = app.slide();
    let selected_object = app.sel.objects.first().cloned();

    if ui.button("+ New trigger").clicked() {
        let event = match &selected_object {
            Some(object) => Event::Click { object: object.clone() },
            None => Event::TimelineStarts,
        };
        triggers.push(Trigger::new(Action::JumpToSlide { target: SlideTarget::Next }, event));
    }
    ui.add_space(4.0);

    let mut remove = None;
    let mut swap = None;
    let count = triggers.len();
    for (i, trigger) in triggers.iter_mut().enumerate() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.checkbox(&mut trigger.enabled, "");
                ui.label(egui::RichText::new(describe(trigger, project, slide)).strong());
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(i > 0, egui::Button::new("⏶").small()).on_hover_text("Run earlier").clicked() {
                    swap = Some((i, i - 1));
                }
                if ui.add_enabled(i + 1 < count, egui::Button::new("⏷").small()).on_hover_text("Run later").clicked()
                {
                    swap = Some((i, i + 1));
                }
                if ui.small_button("Delete").clicked() {
                    remove = Some(i);
                }
            });
            let id = trigger.id.clone();
            ui.push_id(id, |ui| trigger_editor(ui, trigger, project, slide));
        });
        ui.add_space(4.0);
    }
    if let Some(i) = remove {
        triggers.remove(i);
    }
    if let Some((a, b)) = swap {
        triggers.swap(a, b);
    }
    if triggers.is_empty() {
        ui.label(
            egui::RichText::new("No triggers yet. Select an object and add a trigger to make it interactive.").weak(),
        );
    }

    if triggers != app.layer().triggers {
        app.layer_mut().triggers = triggers;
    }
}

fn trigger_editor(ui: &mut Ui, t: &mut Trigger, project: &Project, slide: &Slide) {
    egui::Grid::new("trigger").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Action");
        ComboBox::from_id_salt("action").selected_text(t.action.label()).width(180.0).show_ui(ui, |ui| {
            for template in action_templates(project, slide) {
                let same = discriminant(&template) == discriminant(&t.action);
                if ui.selectable_label(same, template.label()).clicked() && !same {
                    t.action = template;
                }
            }
        });
        ui.end_row();
        action_params(ui, &mut t.action, project, slide);

        ui.label("When");
        ComboBox::from_id_salt("event").selected_text(t.event.label()).width(180.0).show_ui(ui, |ui| {
            for template in event_templates(project, slide) {
                let same = discriminant(&template) == discriminant(&t.event);
                if ui.selectable_label(same, template.label()).clicked() && !same {
                    t.event = template;
                }
            }
        });
        ui.end_row();
        event_params(ui, &mut t.event, project, slide);
    });

    conditions_editor(ui, &mut t.conditions, project, slide);
}

fn action_params(ui: &mut Ui, action: &mut Action, project: &Project, slide: &Slide) {
    match action {
        Action::JumpToSlide { target } => {
            ui.label("Slide");
            slide_target_picker(ui, "slide", project, target);
            ui.end_row();
        }
        Action::ShowLayer { layer } => {
            ui.label("Layer");
            layer_picker(ui, "layer", slide, layer);
            ui.end_row();
        }
        Action::HideLayer { layer }
        | Action::PauseTimeline { layer }
        | Action::ResumeTimeline { layer }
        | Action::RestartTimeline { layer } => {
            ui.label("Layer");
            layer_target_picker(ui, "layer-target", slide, layer);
            ui.end_row();
        }
        Action::ChangeState { object, state } => {
            ui.label("Object");
            object_picker(ui, "state-object", slide, object);
            ui.end_row();
            ui.label("To state");
            state_picker(ui, "state", slide.find_object(object).map(|(_, o)| o), state);
            ui.end_row();
        }
        Action::SetVariable { variable, op, value } => {
            ui.label("Variable");
            variable_picker(ui, "set-var", project, variable);
            ui.end_row();
            let kind = project.variable(variable).map_or(ValueKind::Number, |v| v.value.kind());
            ui.label("Operator");
            ComboBox::from_id_salt("op").selected_text(op.label()).show_ui(ui, |ui| {
                for o in VarOp::ALL {
                    ui.selectable_value(op, o, o.label());
                }
            });
            ui.end_row();
            if *op != VarOp::Toggle {
                ui.label("Value");
                ui.horizontal(|ui| operand_editor(ui, "set-value", project, value, kind));
                ui.end_row();
            }
        }
        Action::OpenUrl { url } => {
            ui.label("URL");
            ui.add(egui::TextEdit::singleline(url).hint_text("https://"));
            ui.end_row();
        }
        Action::CompleteCourse
        | Action::ExitCourse
        | Action::SubmitInteraction
        | Action::SubmitResults
        | Action::ResetResults => {}
    }
}

fn event_params(ui: &mut Ui, event: &mut Event, project: &Project, slide: &Slide) {
    match event {
        Event::Click { object } | Event::HoverStart { object } | Event::HoverEnd { object } => {
            ui.label("Object");
            object_picker(ui, "event-object", slide, object);
            ui.end_row();
        }
        Event::VariableChanges { variable } => {
            ui.label("Variable");
            variable_picker(ui, "event-var", project, variable);
            ui.end_row();
        }
        Event::KeyPressed { key } => {
            ui.label("Key");
            ui.add(egui::TextEdit::singleline(key).desired_width(100.0)).on_hover_text(
                "A character (a, 1, ?) or a key name: Enter, Escape, ArrowLeft, ArrowRight, Tab, \" \" for space",
            );
            ui.end_row();
        }
        Event::TimelineStarts | Event::TimelineEnds => {}
    }
}

fn conditions_editor(ui: &mut Ui, clauses: &mut Vec<ConditionClause>, project: &Project, slide: &Slide) {
    let mut remove = None;
    for (i, clause) in clauses.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            ui.horizontal_wrapped(|ui| {
                if i == 0 {
                    ui.label("If");
                } else {
                    ComboBox::from_id_salt("join")
                        .selected_text(if clause.join == Join::And { "and" } else { "or" })
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut clause.join, Join::And, "and");
                            ui.selectable_value(&mut clause.join, Join::Or, "or");
                        });
                }
                condition_editor(ui, &mut clause.condition, project, slide);
                if ui.small_button("✖").on_hover_text("Remove condition").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        clauses.remove(i);
    }
    if ui.small_button("+ Condition").clicked() {
        let condition = match project.variables.first() {
            Some(v) => Condition::Variable {
                variable: v.id.clone(),
                cmp: Comparison::Eq,
                value: Operand::Literal(v.value.kind().default_value()),
            },
            None => {
                Condition::ObjectState { object: first_object(slide), state: state_names::NORMAL.into(), negate: false }
            }
        };
        clauses.push(ConditionClause { join: Join::And, condition });
    }
}

fn condition_editor(ui: &mut Ui, condition: &mut Condition, project: &Project, slide: &Slide) {
    let is_var = matches!(condition, Condition::Variable { .. });
    let is_state = matches!(condition, Condition::ObjectState { .. });
    let is_quiz = matches!(condition, Condition::QuizPassed { .. });
    let kind_label = if is_var {
        "variable"
    } else if is_state {
        "state of"
    } else {
        "the quiz"
    };
    ComboBox::from_id_salt("cond-kind").selected_text(kind_label).width(80.0).show_ui(ui, |ui| {
        if ui.selectable_label(is_var, "variable").clicked() && !is_var {
            if let Some(v) = project.variables.first() {
                *condition = Condition::Variable {
                    variable: v.id.clone(),
                    cmp: Comparison::Eq,
                    value: Operand::Literal(v.value.kind().default_value()),
                };
            }
        }
        if ui.selectable_label(is_state, "state of").clicked() && !is_state {
            *condition = Condition::ObjectState {
                object: first_object(slide),
                state: state_names::NORMAL.into(),
                negate: false,
            };
        }
        if ui.selectable_label(is_quiz, "the quiz").clicked() && !is_quiz {
            *condition = Condition::QuizPassed { negate: false };
        }
    });
    match condition {
        Condition::Variable { variable, cmp, value } => {
            variable_picker(ui, "cond-var", project, variable);
            ComboBox::from_id_salt("cmp").selected_text(cmp.label()).width(70.0).show_ui(ui, |ui| {
                for c in Comparison::ALL {
                    ui.selectable_value(cmp, c, c.label());
                }
            });
            let kind = project.variable(variable).map_or(ValueKind::Number, |v| v.value.kind());
            operand_editor(ui, "cond-value", project, value, kind);
        }
        Condition::ObjectState { object, state, negate } => {
            object_picker(ui, "cond-object", slide, object);
            ComboBox::from_id_salt("negate").selected_text(if *negate { "is not" } else { "is" }).width(60.0).show_ui(
                ui,
                |ui| {
                    ui.selectable_value(negate, false, "is");
                    ui.selectable_value(negate, true, "is not");
                },
            );
            state_picker(ui, "cond-state", slide.find_object(object).map(|(_, o)| o), state);
        }
        Condition::QuizPassed { negate } => {
            ComboBox::from_id_salt("quiz-passed")
                .selected_text(if *negate { "is failed" } else { "is passed" })
                .width(80.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(negate, false, "is passed");
                    ui.selectable_value(negate, true, "is failed");
                });
        }
    }
}

fn first_object(slide: &Slide) -> Id {
    slide.layers().flat_map(|l| l.objects.iter()).next().map(|o| o.id.clone()).unwrap_or_default()
}

/// One example of every action, with defaults that make sense on `slide`.
fn action_templates(project: &Project, slide: &Slide) -> Vec<Action> {
    let first_layer = slide.layers.first().map(|l| l.id.clone()).unwrap_or_default();
    let first_var = project.variables.first();
    vec![
        Action::JumpToSlide { target: SlideTarget::Next },
        Action::ShowLayer { layer: first_layer },
        Action::HideLayer { layer: LayerTarget::This },
        Action::ChangeState { object: first_object(slide), state: state_names::NORMAL.into() },
        Action::SetVariable {
            variable: first_var.map(|v| v.id.clone()).unwrap_or_default(),
            op: VarOp::Assign,
            value: Operand::Literal(first_var.map_or(Value::Number(0.0), |v| v.value.kind().default_value())),
        },
        Action::PauseTimeline { layer: LayerTarget::This },
        Action::ResumeTimeline { layer: LayerTarget::This },
        Action::RestartTimeline { layer: LayerTarget::This },
        Action::OpenUrl { url: "https://".into() },
        Action::CompleteCourse,
        Action::ExitCourse,
        Action::SubmitInteraction,
        Action::SubmitResults,
        Action::ResetResults,
    ]
}

fn event_templates(project: &Project, slide: &Slide) -> Vec<Event> {
    let object = first_object(slide);
    vec![
        Event::TimelineStarts,
        Event::TimelineEnds,
        Event::Click { object: object.clone() },
        Event::HoverStart { object: object.clone() },
        Event::HoverEnd { object },
        Event::VariableChanges { variable: project.variables.first().map(|v| v.id.clone()).unwrap_or_default() },
        Event::KeyPressed { key: "Enter".into() },
    ]
}

/// A one-line, human-readable summary of a trigger.
fn describe(t: &Trigger, project: &Project, slide: &Slide) -> String {
    let object = |id: &Id| slide.find_object(id).map_or("(missing object)".to_owned(), |(_, o)| o.name.clone());
    let layer = |id: &Id| slide.layer(id).map_or("(missing layer)".to_owned(), |l| l.name.clone());
    let layer_target = |t: &LayerTarget| match t {
        LayerTarget::This => "this layer".to_owned(),
        LayerTarget::Layer(id) => layer(id),
    };
    let var = |id: &Id| project.variable(id).map_or("(missing variable)".to_owned(), |v| v.name.clone());

    let action = match &t.action {
        Action::JumpToSlide { target } => match target {
            SlideTarget::Next => "Jump to the next slide".to_owned(),
            SlideTarget::Previous => "Jump to the previous slide".to_owned(),
            SlideTarget::Slide(id) => {
                format!("Jump to {}", project.slide(id).map_or("(missing slide)".into(), |s| s.name.clone()))
            }
        },
        Action::ShowLayer { layer: l } => format!("Show layer {}", layer(l)),
        Action::HideLayer { layer: l } => format!("Hide {}", layer_target(l)),
        Action::ChangeState { object: o, state } => format!("Change {} to {state}", object(o)),
        Action::SetVariable { variable, op, value } => {
            let rhs = match value {
                Operand::Literal(v) => format!("{v}"),
                Operand::Variable(id) => var(id),
            };
            match op {
                VarOp::Assign => format!("Set {} = {rhs}", var(variable)),
                VarOp::Add => format!("Add {rhs} to {}", var(variable)),
                VarOp::Subtract => format!("Subtract {rhs} from {}", var(variable)),
                VarOp::Multiply => format!("Multiply {} by {rhs}", var(variable)),
                VarOp::Divide => format!("Divide {} by {rhs}", var(variable)),
                VarOp::Toggle => format!("Toggle {}", var(variable)),
            }
        }
        Action::PauseTimeline { layer: l } => format!("Pause timeline of {}", layer_target(l)),
        Action::ResumeTimeline { layer: l } => format!("Resume timeline of {}", layer_target(l)),
        Action::RestartTimeline { layer: l } => format!("Restart timeline of {}", layer_target(l)),
        Action::OpenUrl { url } => format!("Open {url}"),
        Action::CompleteCourse => "Complete the course".to_owned(),
        Action::SubmitInteraction => "Submit the answer".to_owned(),
        Action::SubmitResults => "Submit the quiz results".to_owned(),
        Action::ResetResults => "Reset the quiz".to_owned(),
        Action::ExitCourse => "Exit the course".to_owned(),
    };
    let event = match &t.event {
        Event::TimelineStarts => "when the timeline starts".to_owned(),
        Event::TimelineEnds => "when the timeline ends".to_owned(),
        Event::Click { object: o } => format!("when {} is clicked", object(o)),
        Event::HoverStart { object: o } => format!("when the mouse hovers over {}", object(o)),
        Event::HoverEnd { object: o } => format!("when the mouse leaves {}", object(o)),
        Event::VariableChanges { variable } => format!("when {} changes", var(variable)),
        Event::KeyPressed { key } => format!("when the user presses {}", if key == " " { "Space" } else { key }),
    };
    let conditions = if t.conditions.is_empty() { "" } else { " (if conditions hold)" };
    format!("{action} {event}{conditions}")
}
