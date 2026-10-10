//! A renderer-neutral description of what is on screen.

use serde::{Deserialize, Serialize};
use storycraft_model::{Color, HAlign, Id, Object, ObjectKind, Project, Rect, Slide, Stroke, VAlign, state_names};

use crate::{Engine, substitute_variables};

/// Everything a renderer needs to draw one moment of a slide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub slide: Id,
    pub width: f32,
    pub height: f32,
    pub background: Color,
    /// Bottom to top.
    pub objects: Vec<RenderObject>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// 1-based slide number and total slides.
    pub position: (usize, usize),
    pub completed: bool,
}

/// An object with its current state already applied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderObject {
    pub id: Id,
    pub kind: ObjectKind,
    pub frame: Rect,
    pub rotation: f32,
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
    pub corner_radius: f32,
    pub opacity: f32,
    pub text: Option<RenderText>,
    /// Asset id of the image to draw, for image objects.
    pub image: Option<Id>,
    /// Show a pointer cursor and expose the object to keyboard users.
    pub interactive: bool,
    pub state: String,
    pub alt_text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderText {
    pub text: String,
    pub font_family: String,
    pub font_size: f32,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
    pub align: HAlign,
    pub valign: VAlign,
}

impl RenderObject {
    /// Apply `state` to `object`. `lookup` resolves `%Name%` text references;
    /// pass `|_| None` to show references literally (as the editor does).
    pub fn resolve(object: &Object, state: &str, interactive: bool, lookup: impl Fn(&str) -> Option<String>) -> Self {
        let overrides = object.state(state).map(|s| &s.overrides);
        let style = &object.style;
        let stroke = style.stroke.map(|mut s| {
            if let Some(c) = overrides.and_then(|o| o.stroke_color) {
                s.color = c;
            }
            s
        });
        let text = object.text.as_ref().map(|t| {
            let raw = overrides.and_then(|o| o.text.as_deref()).unwrap_or(&t.text);
            RenderText {
                text: substitute_variables(raw, &lookup),
                font_family: t.font_family.clone(),
                font_size: t.font_size,
                color: overrides.and_then(|o| o.text_color).unwrap_or(t.color),
                bold: t.bold,
                italic: t.italic,
                align: t.align,
                valign: t.valign,
            }
        });
        let image = match &object.kind {
            ObjectKind::Image { asset } => {
                Some(overrides.and_then(|o| o.image.clone()).unwrap_or_else(|| asset.clone()))
            }
            _ => None,
        };
        RenderObject {
            id: object.id.clone(),
            kind: object.kind.clone(),
            frame: object.frame,
            rotation: object.rotation,
            fill: overrides.and_then(|o| o.fill).or(style.fill),
            stroke,
            corner_radius: style.corner_radius,
            opacity: overrides.and_then(|o| o.opacity).unwrap_or(style.opacity).clamp(0.0, 1.0),
            text,
            image,
            interactive,
            state: state.to_owned(),
            alt_text: object.alt_text.clone(),
        }
    }
}

impl Frame {
    /// A static frame for the editor canvas: every object of the given layers
    /// in its Normal state (or `state_override` for one object), regardless of
    /// timing.
    pub fn for_editing(project: &Project, slide: &Slide, layers: &[Id], state_override: Option<(&Id, &str)>) -> Frame {
        let objects = slide
            .layers()
            .filter(|l| layers.contains(&l.id))
            .flat_map(|l| l.objects.iter())
            .map(|o| {
                let state = match state_override {
                    Some((id, s)) if id == &o.id => s,
                    _ => state_names::NORMAL,
                };
                RenderObject::resolve(o, state, false, |_| None)
            })
            .collect();
        Frame {
            slide: slide.id.clone(),
            width: project.slide_size.width,
            height: project.slide_size.height,
            background: slide.background,
            objects,
            can_go_back: false,
            can_go_forward: false,
            position: (0, 0),
            completed: false,
        }
    }
}

pub(crate) fn frame_for_engine(engine: &Engine) -> Frame {
    let project = engine.project();
    let lookup = |name: &str| {
        if let Some(value) = engine.results_variable(name) {
            return Some(value);
        }
        let var = project.variable_by_name(name)?;
        engine.variable(&var.id).map(|v| v.to_string())
    };
    let Some(slide) = engine.current_slide() else {
        return Frame {
            slide: Id::from(""),
            width: project.slide_size.width,
            height: project.slide_size.height,
            background: Color::BLACK,
            objects: Vec::new(),
            can_go_back: false,
            can_go_forward: false,
            position: engine.position(),
            completed: engine.is_completed(),
        };
    };
    let reachable: Vec<&Id> = engine.reachable_objects().map(|o| &o.id).collect();
    let objects = engine
        .visible_objects()
        .map(|(_, o)| {
            let interactive = reachable.contains(&&o.id) && engine.is_interactive(o);
            RenderObject::resolve(o, engine.display_state(o), interactive, lookup)
        })
        .collect();
    Frame {
        slide: slide.id.clone(),
        width: project.slide_size.width,
        height: project.slide_size.height,
        background: slide.background,
        objects,
        can_go_back: engine.can_go_back(),
        can_go_forward: engine.can_go_forward(),
        position: engine.position(),
        completed: engine.is_completed(),
    }
}
