//! The slide editing canvas: select, move and resize objects.

use std::collections::HashSet;

use egui::{Color32, CursorIcon, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2};
use storycraft_engine::Frame;
use storycraft_model::{Id, Point};

use crate::app::{EditorApp, RightTab};
use crate::draw::{SlideView, draw_frame, frame_contains};

const HANDLE: f32 = 8.0;
const MIN_SIZE: f32 = 4.0;
const SELECTION: Color32 = Color32::from_rgb(0x1E, 0x90, 0xFF);

#[derive(Default)]
pub struct CanvasState {
    drag: Option<Drag>,
    /// Set by a double-click so the properties panel focuses the text field.
    pub focus_text: bool,
}

enum Drag {
    Move { start: Point, originals: Vec<(Id, storycraft_model::Rect)> },
    Resize { id: Id, handle: Handle, start: Point, original: storycraft_model::Rect },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl Handle {
    const ALL: [Handle; 8] =
        [Handle::NW, Handle::N, Handle::NE, Handle::E, Handle::SE, Handle::S, Handle::SW, Handle::W];

    fn position(self, r: Rect) -> Pos2 {
        match self {
            Handle::N => r.center_top(),
            Handle::S => r.center_bottom(),
            Handle::E => r.right_center(),
            Handle::W => r.left_center(),
            Handle::NE => r.right_top(),
            Handle::NW => r.left_top(),
            Handle::SE => r.right_bottom(),
            Handle::SW => r.left_bottom(),
        }
    }

    fn cursor(self) -> CursorIcon {
        match self {
            Handle::N | Handle::S => CursorIcon::ResizeVertical,
            Handle::E | Handle::W => CursorIcon::ResizeHorizontal,
            Handle::NE | Handle::SW => CursorIcon::ResizeNeSw,
            Handle::NW | Handle::SE => CursorIcon::ResizeNwSe,
        }
    }

    fn is_corner(self) -> bool {
        matches!(self, Handle::NE | Handle::NW | Handle::SE | Handle::SW)
    }

    /// Apply a pointer movement of (`dx`, `dy`) slide pixels to `r`.
    fn resize(self, r: storycraft_model::Rect, dx: f32, dy: f32, keep_aspect: bool) -> storycraft_model::Rect {
        let (mut left, mut top, mut right, mut bottom) = (r.x, r.y, r.x + r.width, r.y + r.height);
        let west = matches!(self, Handle::W | Handle::NW | Handle::SW);
        let east = matches!(self, Handle::E | Handle::NE | Handle::SE);
        let north = matches!(self, Handle::N | Handle::NE | Handle::NW);
        let south = matches!(self, Handle::S | Handle::SE | Handle::SW);
        if west {
            left = (left + dx).min(right - MIN_SIZE);
        }
        if east {
            right = (right + dx).max(left + MIN_SIZE);
        }
        if north {
            top = (top + dy).min(bottom - MIN_SIZE);
        }
        if south {
            bottom = (bottom + dy).max(top + MIN_SIZE);
        }
        if keep_aspect && self.is_corner() && r.width > 0.0 && r.height > 0.0 {
            let height = (right - left) * r.height / r.width;
            if north {
                top = bottom - height;
            } else {
                bottom = top + height;
            }
        }
        storycraft_model::Rect::new(left.round(), top.round(), (right - left).round(), (bottom - top).round())
    }
}

pub fn show(app: &mut EditorApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    painter.rect_filled(response.rect, 0.0, ui.visuals().extreme_bg_color);

    let size = app.project().slide_size;
    let view = SlideView::fit(response.rect, size, 24.0);
    let slide_rect = view.slide_rect(size);

    // Draw: the base layer always, plus the layer being edited. Objects not on
    // the edited layer are dimmed, and initially hidden ones are ghosted.
    let mut layers = vec![app.slide().base.id.clone()];
    if !app.is_base_layer() {
        layers.push(app.sel.layer.clone());
    }
    let frame = Frame::for_editing(app.project(), app.slide(), &layers, None);
    let editable: HashSet<Id> = app.layer().objects.iter().map(|o| o.id.clone()).collect();
    let ghosted: HashSet<Id> = app
        .slide()
        .layers()
        .flat_map(|l| l.objects.iter())
        .filter(|o| o.initially_hidden)
        .map(|o| o.id.clone())
        .collect();
    {
        let textures = &mut app.textures;
        let media = &app.doc.media;
        draw_frame(&painter, view, &frame, &mut |id| textures.get(&ctx, id, media), &|id| {
            let base = if editable.contains(id) { 1.0 } else { 0.35 };
            if ghosted.contains(id) { base * 0.5 } else { base }
        });
    }
    painter.rect_stroke(slide_rect, 0.0, Stroke::new(1.0, Color32::from_gray(110)), StrokeKind::Outside);

    if app.layer().objects.is_empty() {
        painter.text(
            slide_rect.center_bottom() - vec2(0.0, 16.0),
            egui::Align2::CENTER_BOTTOM,
            "Add text, shapes, buttons and pictures from the toolbar or the Insert menu",
            egui::FontId::proportional(13.0),
            Color32::from_gray(140),
        );
    }

    // Selection outlines and handles.
    let primary_rect = app.selected().map(|o| view.rect(&o.frame));
    for id in &app.sel.objects {
        if let Some(o) = app.layer().objects.iter().find(|o| &o.id == id) {
            painter.rect_stroke(view.rect(&o.frame), 0.0, Stroke::new(1.5, SELECTION), StrokeKind::Outside);
        }
    }
    if let (Some(r), true) = (primary_rect, app.sel.objects.len() == 1) {
        for h in Handle::ALL {
            let hr = Rect::from_center_size(h.position(r), vec2(HANDLE, HANDLE));
            painter.rect(hr, 1.0, Color32::WHITE, Stroke::new(1.0, SELECTION), StrokeKind::Middle);
        }
    }

    // --- interaction ---------------------------------------------------------
    let pointer = response.hover_pos().or_else(|| response.interact_pointer_pos());
    let single = app.sel.objects.len() == 1;
    let handle_at = |p: Pos2| -> Option<Handle> {
        let r = primary_rect.filter(|_| single)?;
        Handle::ALL
            .into_iter()
            .find(|h| Rect::from_center_size(h.position(r), vec2(HANDLE + 4.0, HANDLE + 4.0)).contains(p))
    };
    let object_at = |app: &EditorApp, p: Pos2| -> Option<Id> {
        let sp = view.to_slide(p);
        app.layer().objects.iter().rev().find(|o| frame_contains(&o.frame, o.rotation, sp)).map(|o| o.id.clone())
    };

    if let Some(p) = pointer {
        if app.canvas.drag.is_none() {
            if let Some(h) = handle_at(p) {
                ctx.set_cursor_icon(h.cursor());
            } else if object_at(app, p).is_some() {
                ctx.set_cursor_icon(CursorIcon::Move);
            }
        }
    }

    let shift = ctx.input(|i| i.modifiers.shift);
    if response.drag_started() {
        if let Some(p) = response.interact_pointer_pos() {
            let start = view.to_slide(p);
            if let Some(handle) = handle_at(p) {
                let o = app.selected().unwrap();
                app.canvas.drag = Some(Drag::Resize { id: o.id.clone(), handle, start, original: o.frame });
            } else if let Some(id) = object_at(app, p) {
                if !app.sel.objects.contains(&id) {
                    if shift {
                        app.sel.objects.push(id);
                    } else {
                        app.sel.objects = vec![id];
                    }
                }
                let originals = app
                    .layer()
                    .objects
                    .iter()
                    .filter(|o| app.sel.objects.contains(&o.id))
                    .map(|o| (o.id.clone(), o.frame))
                    .collect();
                app.canvas.drag = Some(Drag::Move { start, originals });
            } else {
                app.sel.objects.clear();
            }
        }
    }
    if response.dragged() {
        if let (Some(p), Some(drag)) = (response.interact_pointer_pos(), &app.canvas.drag) {
            let now = view.to_slide(p);
            match drag {
                Drag::Move { start, originals } => {
                    let (dx, dy) = ((now.x - start.x).round(), (now.y - start.y).round());
                    let originals = originals.clone();
                    for o in app.layer_mut().objects.iter_mut() {
                        if let Some((_, r)) = originals.iter().find(|(id, _)| id == &o.id) {
                            o.frame.x = r.x + dx;
                            o.frame.y = r.y + dy;
                        }
                    }
                }
                Drag::Resize { id, handle, start, original } => {
                    let r = handle.resize(*original, now.x - start.x, now.y - start.y, shift);
                    let id = id.clone();
                    if let Some(o) = app.layer_mut().objects.iter_mut().find(|o| o.id == id) {
                        o.frame = r;
                    }
                }
            }
        }
    }
    if response.drag_stopped() {
        app.canvas.drag = None;
    }

    if response.clicked() {
        if let Some(p) = response.interact_pointer_pos() {
            match object_at(app, p) {
                Some(id) if shift => {
                    if let Some(i) = app.sel.objects.iter().position(|x| x == &id) {
                        app.sel.objects.remove(i);
                    } else {
                        app.sel.objects.push(id);
                    }
                }
                Some(id) => app.sel.objects = vec![id],
                None => app.sel.objects.clear(),
            }
            app.right_tab = RightTab::Properties;
        }
    }
    if response.double_clicked() && app.selected().is_some_and(|o| o.text.is_some()) {
        app.canvas.focus_text = true;
        app.right_tab = RightTab::Properties;
    }

    response.context_menu(|ui| {
        let has_sel = !app.sel.objects.is_empty();
        if ui.add_enabled(has_sel, egui::Button::new("Copy")).clicked() {
            app.copy_selected();
        }
        if ui.add_enabled(!app.clipboard.is_empty(), egui::Button::new("Paste")).clicked() {
            app.paste();
        }
        if ui.add_enabled(has_sel, egui::Button::new("Duplicate")).clicked() {
            app.duplicate_selected();
        }
        if ui.add_enabled(has_sel, egui::Button::new("Delete")).clicked() {
            app.delete_selected();
        }
        ui.separator();
        if ui.add_enabled(has_sel, egui::Button::new("Bring forward")).clicked() {
            app.reorder_selected(true);
        }
        if ui.add_enabled(has_sel, egui::Button::new("Send backward")).clicked() {
            app.reorder_selected(false);
        }
        ui.separator();
        ui.menu_button("Insert", |ui| app.insert_menu(ui));
    });

    keyboard(app, &ctx);
}

/// Canvas keyboard commands. Skipped while a text field has focus.
fn keyboard(app: &mut EditorApp, ctx: &egui::Context) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let (nudge, delete, escape, copy, paste, duplicate, select_all) = ctx.input_mut(|i| {
        let step = if i.modifiers.shift { 10.0 } else { 1.0 };
        let mut nudge = (0.0, 0.0);
        for (key, d) in [
            (Key::ArrowLeft, (-step, 0.0)),
            (Key::ArrowRight, (step, 0.0)),
            (Key::ArrowUp, (0.0, -step)),
            (Key::ArrowDown, (0.0, step)),
        ] {
            if i.key_pressed(key) {
                nudge = (nudge.0 + d.0, nudge.1 + d.1);
            }
        }
        let cmd = |key| egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key);
        (
            nudge,
            i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
            i.key_pressed(Key::Escape),
            i.consume_shortcut(&cmd(Key::C)) || i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
            i.consume_shortcut(&cmd(Key::V)) || i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
            i.consume_shortcut(&cmd(Key::D)),
            i.consume_shortcut(&cmd(Key::A)),
        )
    });
    if nudge != (0.0, 0.0) {
        let ids = app.sel.objects.clone();
        for o in app.layer_mut().objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            o.frame.x += nudge.0;
            o.frame.y += nudge.1;
        }
    }
    if delete {
        app.delete_selected();
    }
    if escape {
        app.sel.objects.clear();
    }
    if copy {
        app.copy_selected();
    }
    if paste {
        app.paste();
    }
    if duplicate {
        app.duplicate_selected();
    }
    if select_all {
        app.sel.objects = app.layer().objects.iter().map(|o| o.id.clone()).collect();
    }
}
