//! Paints engine [`Frame`]s with egui. Used by both the edit canvas and the
//! preview, so what authors see while editing matches what learners get.

use egui::epaint::{Mesh, TextShape};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, TextureId, Vec2, emath::Rot2, pos2, vec2};
use storycraft_engine::{Frame, RenderObject};
use storycraft_model::{Color, HAlign, Id, ObjectKind, Point, ShapeKind, Size, VAlign};

/// Text inset inside its object, matching the web player's CSS padding.
const TEXT_PAD: Vec2 = vec2(8.0, 4.0);

/// Maps slide coordinates onto the screen.
#[derive(Clone, Copy, Debug)]
pub struct SlideView {
    pub origin: Pos2,
    pub scale: f32,
}

impl SlideView {
    /// Fit a slide of `size` inside `available`, centred, leaving `margin`.
    pub fn fit(available: Rect, size: Size, margin: f32) -> Self {
        let room = (available.size() - Vec2::splat(margin * 2.0)).max(Vec2::splat(1.0));
        let scale = (room.x / size.width).min(room.y / size.height).max(0.01);
        let used = vec2(size.width, size.height) * scale;
        SlideView { origin: available.center() - used / 2.0, scale }
    }

    pub fn to_screen(self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y) * self.scale
    }

    pub fn to_slide(self, p: Pos2) -> Point {
        let v = (p - self.origin) / self.scale;
        Point { x: v.x, y: v.y }
    }

    pub fn rect(self, r: &storycraft_model::Rect) -> Rect {
        Rect::from_min_size(self.to_screen(r.x, r.y), vec2(r.width, r.height) * self.scale)
    }

    pub fn slide_rect(self, size: Size) -> Rect {
        Rect::from_min_size(self.origin, vec2(size.width, size.height) * self.scale)
    }
}

pub fn color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

pub fn from_color32(c: Color32) -> Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color::rgba(r, g, b, a)
}

fn faded(c: Color32, opacity: f32) -> Color32 {
    c.gamma_multiply(opacity)
}

/// Paint a whole frame: background, then objects bottom to top.
/// `opacity_of` lets the editor dim objects that are not being edited.
pub fn draw_frame(
    painter: &Painter,
    view: SlideView,
    frame: &Frame,
    texture: &mut dyn FnMut(&Id) -> Option<TextureId>,
    opacity_of: &dyn Fn(&Id) -> f32,
) {
    let slide = view.slide_rect(Size { width: frame.width, height: frame.height });
    painter.rect_filled(slide, 0.0, color32(frame.background));
    let clipped = painter.with_clip_rect(slide.intersect(painter.clip_rect()));
    for object in &frame.objects {
        let tex = object.image.as_ref().and_then(&mut *texture);
        draw_object(&clipped, view, object, opacity_of(&object.id), tex);
    }
}

pub fn draw_object(painter: &Painter, view: SlideView, o: &RenderObject, opacity: f32, texture: Option<TextureId>) {
    let opacity = o.opacity * opacity;
    if opacity <= 0.0 {
        return;
    }
    let rect = view.rect(&o.frame);
    let center = rect.center();
    let rot = Rot2::from_angle(o.rotation.to_radians());
    let rotate = |points: Vec<Pos2>| -> Vec<Pos2> { points.into_iter().map(|p| center + rot * (p - center)).collect() };

    let outline = |r: Rect| -> Vec<Pos2> {
        match &o.kind {
            ObjectKind::Shape { shape: ShapeKind::Ellipse } => ellipse_points(r),
            ObjectKind::Shape { shape: ShapeKind::Triangle } => vec![r.center_top(), r.right_bottom(), r.left_bottom()],
            _ => rounded_rect_points(r, (o.corner_radius * view.scale).min(r.width() / 2.0).min(r.height() / 2.0)),
        }
    };

    if let Some(fill) = o.fill {
        painter.add(Shape::convex_polygon(rotate(outline(rect)), faded(color32(fill), opacity), Stroke::NONE));
    }

    if let (Some(tex), ObjectKind::Image { .. }) = (texture, &o.kind) {
        let mut mesh = Mesh::with_texture(tex);
        mesh.add_rect_with_uv(rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), faded(Color32::WHITE, opacity));
        mesh.rotate(rot, center);
        painter.add(Shape::mesh(mesh));
    } else if matches!(o.kind, ObjectKind::Image { .. }) {
        // Missing or undecodable image: draw a placeholder.
        let points = rotate(rounded_rect_points(rect, 0.0));
        painter.add(Shape::convex_polygon(
            points.clone(),
            Color32::from_gray(200).gamma_multiply(opacity),
            Stroke::NONE,
        ));
        painter.add(Shape::closed_line(points, Stroke::new(1.0, Color32::from_gray(120))));
    }

    if let ObjectKind::Web { source, display } = &o.kind {
        // The editor cannot render web pages; show what will load there.
        let points = rotate(rounded_rect_points(rect, 0.0));
        painter.add(Shape::convex_polygon(
            points.clone(),
            Color32::from_gray(235).gamma_multiply(opacity),
            Stroke::NONE,
        ));
        painter.add(Shape::closed_line(points, Stroke::new(1.0, Color32::from_gray(140))));
        let what = match source {
            storycraft_model::WebSource::Url { url } => url.clone(),
            storycraft_model::WebSource::Bundle { entry, .. } => format!("Local folder: {entry}"),
        };
        let mode = match display {
            storycraft_model::WebDisplay::OnSlide => "Web object",
            storycraft_model::WebDisplay::NewWindow => "Web object (opens in a new window)",
        };
        let size = (14.0 * view.scale).clamp(8.0, 18.0);
        let clipped = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
        clipped.text(
            rect.center() - vec2(0.0, size * 0.7),
            egui::Align2::CENTER_CENTER,
            mode,
            FontId::proportional(size),
            Color32::from_gray(60).gamma_multiply(opacity),
        );
        clipped.text(
            rect.center() + vec2(0.0, size * 0.7),
            egui::Align2::CENTER_CENTER,
            what,
            FontId::proportional(size * 0.85),
            Color32::from_rgb(0x1D, 0x4E, 0xD8).gamma_multiply(opacity),
        );
    }

    if let Some(stroke) = o.stroke.filter(|s| s.width > 0.0) {
        // Strokes sit inside the frame, like CSS `box-sizing: border-box`.
        let width = stroke.width * view.scale;
        let inner = rect.shrink(width / 2.0);
        if inner.is_positive() {
            painter.add(Shape::closed_line(
                rotate(outline(inner)),
                Stroke::new(width, faded(color32(stroke.color), opacity)),
            ));
        }
    }

    if let Some(text) = &o.text {
        if !text.text.is_empty() {
            draw_text(painter, view, rect, o.rotation, text, opacity);
        }
    }
}

fn draw_text(
    painter: &Painter,
    view: SlideView,
    rect: Rect,
    rotation: f32,
    t: &storycraft_engine::RenderText,
    opacity: f32,
) {
    let content = rect.shrink2(TEXT_PAD * view.scale);
    if content.width() <= 1.0 {
        return;
    }
    let color = faded(color32(t.color), opacity);
    let mut job = LayoutJob::single_section(
        t.text.clone(),
        TextFormat {
            font_id: FontId::proportional((t.font_size * view.scale).max(1.0)),
            color,
            italics: t.italic,
            ..Default::default()
        },
    );
    job.wrap.max_width = content.width();
    job.halign = match t.align {
        HAlign::Left => Align::LEFT,
        HAlign::Center => Align::Center,
        HAlign::Right => Align::RIGHT,
    };
    let galley = painter.layout_job(job);
    let x = match t.align {
        HAlign::Left => content.left(),
        HAlign::Center => content.center().x,
        HAlign::Right => content.right(),
    };
    let height = galley.size().y;
    let y = match t.valign {
        VAlign::Top => content.top(),
        VAlign::Middle => content.center().y - height / 2.0,
        VAlign::Bottom => content.bottom() - height,
    };
    let mut pos = pos2(x, y);
    let angle = rotation.to_radians();
    if angle != 0.0 {
        let center = rect.center();
        pos = center + Rot2::from_angle(angle) * (pos - center);
    }
    let clipped =
        if angle == 0.0 { painter.with_clip_rect(rect.intersect(painter.clip_rect())) } else { painter.clone() };
    let mut shape = TextShape::new(pos, galley, color);
    shape.angle = angle;
    if t.bold {
        // egui's default fonts have no bold face; overdraw with a small offset.
        let mut bold = shape.clone();
        bold.pos += Rot2::from_angle(angle) * vec2((t.font_size * view.scale * 0.035).max(0.6), 0.0);
        clipped.add(bold);
    }
    clipped.add(shape);
}

fn rounded_rect_points(r: Rect, radius: f32) -> Vec<Pos2> {
    if radius <= 0.5 {
        return vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
    }
    const STEPS: usize = 8;
    let corners = [
        (pos2(r.right() - radius, r.top() + radius), -90.0f32),
        (pos2(r.right() - radius, r.bottom() - radius), 0.0),
        (pos2(r.left() + radius, r.bottom() - radius), 90.0),
        (pos2(r.left() + radius, r.top() + radius), 180.0),
    ];
    let mut points = Vec::with_capacity(4 * (STEPS + 1));
    for (c, start) in corners {
        for i in 0..=STEPS {
            let a = (start + 90.0 * i as f32 / STEPS as f32).to_radians();
            points.push(c + vec2(a.cos(), a.sin()) * radius);
        }
    }
    points
}

fn ellipse_points(r: Rect) -> Vec<Pos2> {
    const STEPS: usize = 64;
    let (c, rx, ry) = (r.center(), r.width() / 2.0, r.height() / 2.0);
    (0..STEPS)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / STEPS as f32;
            c + vec2(a.cos() * rx, a.sin() * ry)
        })
        .collect()
}

/// Whether slide point `p` falls inside a (possibly rotated) object frame.
pub fn frame_contains(frame: &storycraft_model::Rect, rotation: f32, p: Point) -> bool {
    let c = frame.center();
    let (sin, cos) = (-rotation.to_radians()).sin_cos();
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    frame.contains(Point { x: c.x + dx * cos - dy * sin, y: c.y + dx * sin + dy * cos })
}
