//! Layers list and the timeline of the layer being edited.

use egui::{Color32, CursorIcon, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use storycraft_model::Id;

use crate::app::EditorApp;
use crate::widgets::seconds;

const ROW: f32 = 22.0;
const NAME_COLUMN: f32 = 150.0;
const SNAP_MS: f32 = 100.0;

/// An in-progress drag on a timeline bar.
#[derive(Clone)]
struct BarDrag {
    object: Id,
    /// Dragging the right edge (duration) instead of the whole bar (start).
    edge: bool,
    start_ms: u32,
    duration_ms: Option<u32>,
}

pub fn show(app: &mut EditorApp, ui: &mut Ui) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(220.0);
            layers(app, ui);
        });
        ui.separator();
        ui.vertical(|ui| timeline(app, ui));
    });
}

fn layers(app: &mut EditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.strong("Layers");
        if ui.small_button("+").on_hover_text("New layer").clicked() {
            app.add_layer();
        }
    });
    let entries: Vec<(Id, String, bool)> =
        app.slide().layers().enumerate().map(|(i, l)| (l.id.clone(), l.name.clone(), i == 0)).collect();
    let mut action: Option<(Id, &str)> = None;
    egui::ScrollArea::vertical().id_salt("layers").show(ui, |ui| {
        // Top layer first, as in a drawing stack.
        for (id, name, is_base) in entries.iter().rev() {
            let r = ui.selectable_label(app.sel.layer == *id, name);
            if r.clicked() {
                action = Some((id.clone(), "select"));
            }
            if !is_base {
                r.context_menu(|ui| {
                    if ui.button("Duplicate").clicked() {
                        action = Some((id.clone(), "duplicate"));
                    }
                    if ui.button("Delete").clicked() {
                        action = Some((id.clone(), "delete"));
                    }
                });
            }
        }
    });
    match action {
        Some((id, "select")) => app.select_layer(id),
        Some((id, "duplicate")) => app.duplicate_layer(&id),
        Some((id, "delete")) => app.delete_layer(&id),
        _ => {}
    }
}

fn timeline(app: &mut EditorApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.strong(format!("Timeline: {}", app.layer().name));
        ui.label("Duration");
        seconds(ui, &mut app.layer_mut().duration_ms);
    });

    let duration = app.layer().duration_ms.max(1000) as f32;
    let width = ui.available_width().max(NAME_COLUMN + 100.0);
    let track = width - NAME_COLUMN - 8.0;
    let px_per_ms = track / duration;
    let drag_id = ui.make_persistent_id("timeline-drag");

    // Ruler.
    let (ruler, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
    let painter = ui.painter();
    let step_ms = [500.0, 1000.0, 2000.0, 5000.0, 10_000.0, 30_000.0, 60_000.0]
        .into_iter()
        .find(|s| s * px_per_ms >= 50.0)
        .unwrap_or(60_000.0);
    let mut t = 0.0;
    while t <= duration + 0.5 {
        let x = ruler.left() + NAME_COLUMN + t * px_per_ms;
        painter.line_segment([pos2(x, ruler.bottom() - 5.0), pos2(x, ruler.bottom())], Stroke::new(1.0, Color32::GRAY));
        painter.text(
            pos2(x + 2.0, ruler.top()),
            egui::Align2::LEFT_TOP,
            format!("{}s", t / 1000.0),
            egui::FontId::proportional(10.0),
            ui.visuals().weak_text_color(),
        );
        t += step_ms;
    }

    let selected: Vec<Id> = app.sel.objects.clone();
    let rows: Vec<(Id, String, u32, Option<u32>)> = app
        .layer()
        .objects
        .iter()
        .rev()
        .map(|o| (o.id.clone(), o.name.clone(), o.timing.start_ms, o.timing.duration_ms))
        .collect();
    if rows.is_empty() {
        ui.label(egui::RichText::new("Objects on this layer will appear here.").weak());
    }

    let mut drag: Option<BarDrag> = ui.data(|d| d.get_temp(drag_id));
    egui::ScrollArea::vertical().id_salt("timeline-rows").auto_shrink([false, true]).show(ui, |ui| {
        for (id, name, start, dur) in rows {
            let (row, response) = ui.allocate_exact_size(vec2(width, ROW), Sense::click_and_drag());
            let painter = ui.painter();
            let is_selected = selected.contains(&id);
            if is_selected {
                painter.rect_filled(row, 2.0, ui.visuals().selection.bg_fill.gamma_multiply(0.35));
            }
            painter.text(
                pos2(row.left() + 4.0, row.center().y),
                egui::Align2::LEFT_CENTER,
                &name,
                egui::FontId::proportional(12.0),
                ui.visuals().text_color(),
            );
            let x0 = row.left() + NAME_COLUMN + start as f32 * px_per_ms;
            let end_ms = dur.map_or(duration, |d| start as f32 + d as f32).min(duration.max(start as f32));
            let x1 = (row.left() + NAME_COLUMN + end_ms * px_per_ms).max(x0 + 4.0);
            let bar = Rect::from_min_max(pos2(x0, row.top() + 3.0), pos2(x1, row.bottom() - 3.0));
            let fill =
                if is_selected { Color32::from_rgb(0x1E, 0x90, 0xFF) } else { Color32::from_rgb(0x6A, 0x8C, 0xAF) };
            painter.rect(bar, 3.0, fill, Stroke::NONE, StrokeKind::Inside);
            if dur.is_none() {
                // Open-ended bars get a chevron: "stays until the end".
                let tip = bar.right_center() - vec2(4.0, 0.0);
                painter.add(egui::Shape::convex_polygon(
                    vec![tip + vec2(-6.0, -5.0), tip, tip + vec2(-6.0, 5.0)],
                    Color32::WHITE,
                    Stroke::NONE,
                ));
            }

            let edge_zone =
                Rect::from_min_max(pos2(bar.right() - 6.0, bar.top()), pos2(bar.right() + 4.0, bar.bottom()));
            if let Some(p) = response.hover_pos() {
                if edge_zone.contains(p) {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                } else if bar.contains(p) {
                    ui.ctx().set_cursor_icon(CursorIcon::Grab);
                }
            }
            if response.clicked() || response.drag_started() {
                app.sel.objects = vec![id.clone()];
            }
            if response.drag_started() {
                if let Some(p) = response.interact_pointer_pos() {
                    if bar.contains(p) || edge_zone.contains(p) {
                        drag = Some(BarDrag {
                            object: id.clone(),
                            edge: edge_zone.contains(p),
                            start_ms: start,
                            duration_ms: dur,
                        });
                    }
                }
            }
            if let Some(d) = drag.as_ref().filter(|d| d.object == id) {
                if response.dragged() {
                    let total = response.total_drag_delta().map_or(0.0, |v| v.x) / px_per_ms;
                    let delta = (total / SNAP_MS).round() * SNAP_MS;
                    let d = d.clone();
                    if let Some(o) = app.layer_mut().objects.iter_mut().find(|o| o.id == id) {
                        if d.edge {
                            let base = d.duration_ms.map_or(duration - d.start_ms as f32, |x| x as f32);
                            o.timing.duration_ms = Some((base + delta).max(SNAP_MS) as u32);
                        } else {
                            o.timing.start_ms = (d.start_ms as f32 + delta).clamp(0.0, duration) as u32;
                        }
                    }
                }
            }
            if response.drag_stopped() {
                drag = None;
            }
        }
    });
    ui.data_mut(|d| match drag {
        Some(v) => {
            d.insert_temp(drag_id, v);
        }
        None => {
            d.remove::<BarDrag>(drag_id);
        }
    });
}
