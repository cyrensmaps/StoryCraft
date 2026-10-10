use serde::{Deserialize, Serialize};

use crate::{Color, Id, Rect};

/// Names of the states the runtime understands natively. Any other name is a
/// custom state that only changes when a trigger asks for it.
pub mod state_names {
    /// The implicit state every object starts in. It never needs to be
    /// declared in [`crate::Object::states`].
    pub const NORMAL: &str = "Normal";
    /// Shown automatically while the pointer is over the object.
    pub const HOVER: &str = "Hover";
    /// Shown automatically while the object is being pressed.
    pub const DOWN: &str = "Down";
    /// Entered automatically after the object is clicked, if declared.
    pub const VISITED: &str = "Visited";
    /// Toggled on and off by clicks, if declared.
    pub const SELECTED: &str = "Selected";
    /// Object is shown but ignores the pointer.
    pub const DISABLED: &str = "Disabled";
    /// Object is not shown. Always available, never needs declaring.
    pub const HIDDEN: &str = "Hidden";

    pub const BUILT_IN: [&str; 7] = [NORMAL, HOVER, DOWN, VISITED, SELECTED, DISABLED, HIDDEN];
}

/// Something placed on a slide layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: Id,
    pub name: String,
    pub kind: ObjectKind,
    pub frame: Rect,
    /// Clockwise rotation in degrees around the frame centre.
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub style: Style,
    /// Text drawn inside the object's frame (text boxes, shapes, buttons).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextContent>,
    #[serde(default)]
    pub timing: Timing,
    /// Start in the built-in Hidden state instead of Normal.
    #[serde(default)]
    pub initially_hidden: bool,
    /// Declared states besides Normal and Hidden, in editor order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<ObjectState>,
    /// Text announced by screen readers. Empty means decorative.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub alt_text: String,
}

impl Object {
    pub fn new(name: impl Into<String>, kind: ObjectKind, frame: Rect) -> Self {
        Object {
            id: Id::new(),
            name: name.into(),
            kind,
            frame,
            rotation: 0.0,
            style: Style::default(),
            text: None,
            timing: Timing::default(),
            initially_hidden: false,
            states: Vec::new(),
            alt_text: String::new(),
        }
    }

    pub fn state(&self, name: &str) -> Option<&ObjectState> {
        self.states.iter().find(|s| s.name == name)
    }

    pub fn has_state(&self, name: &str) -> bool {
        name == state_names::NORMAL || name == state_names::HIDDEN || self.state(name).is_some()
    }

    /// Whether the object gives pointer feedback on its own (used to pick a
    /// pointer cursor). Objects with click triggers are interactive too; the
    /// engine knows about those.
    pub fn has_pointer_feedback(&self) -> bool {
        matches!(self.kind, ObjectKind::Button)
            || self.state(state_names::HOVER).is_some()
            || self.state(state_names::DOWN).is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ObjectKind {
    /// A text box. Its style usually has no fill.
    Text,
    Shape {
        shape: ShapeKind,
    },
    Image {
        asset: Id,
    },
    /// A rectangle with button semantics for assistive technology.
    Button,
    /// A web page shown in the slide (or in a new window).
    Web {
        source: WebSource,
        #[serde(default)]
        display: WebDisplay,
    },
}

/// Where a web object's page comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSource {
    /// A page on the web.
    Url { url: String },
    /// A folder of web content packed into a ZIP asset, opened at `entry`
    /// (a path inside the ZIP, e.g. `index.html`).
    Bundle { asset: Id, entry: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WebDisplay {
    /// Embedded in the slide, filling the object's frame.
    #[default]
    OnSlide,
    /// Opened in a new browser window when the object appears.
    NewWindow,
}

impl ObjectKind {
    pub fn label(&self) -> &'static str {
        match self {
            ObjectKind::Text => "Text",
            ObjectKind::Shape { .. } => "Shape",
            ObjectKind::Image { .. } => "Image",
            ObjectKind::Button => "Button",
            ObjectKind::Web { .. } => "Web object",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    #[default]
    Rectangle,
    Ellipse,
    Triangle,
}

impl ShapeKind {
    pub const ALL: [ShapeKind; 3] = [ShapeKind::Rectangle, ShapeKind::Ellipse, ShapeKind::Triangle];

    pub fn label(&self) -> &'static str {
        match self {
            ShapeKind::Rectangle => "Rectangle",
            ShapeKind::Ellipse => "Ellipse",
            ShapeKind::Triangle => "Triangle",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    #[serde(default)]
    pub corner_radius: f32,
    /// 0.0 (invisible) to 1.0 (opaque).
    #[serde(default = "one")]
    pub opacity: f32,
}

fn one() -> f32 {
    1.0
}

impl Default for Style {
    fn default() -> Self {
        Style { fill: None, stroke: None, corner_radius: 0.0, opacity: 1.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// Text inside an object. `%VariableName%` is replaced with the variable's
/// current value at runtime.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    pub font_size: f32,
    pub color: Color,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub align: HAlign,
    #[serde(default)]
    pub valign: VAlign,
}

fn default_font_family() -> String {
    "sans-serif".to_owned()
}

impl TextContent {
    pub fn new(text: impl Into<String>) -> Self {
        TextContent {
            text: text.into(),
            font_family: default_font_family(),
            font_size: 24.0,
            color: Color::BLACK,
            bold: false,
            italic: false,
            align: HAlign::default(),
            valign: VAlign::default(),
        }
    }
}

/// When an object is on screen, relative to the start of its layer's
/// timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Timing {
    pub start_ms: u32,
    /// `None` means "until the end of the timeline" (and beyond: objects stay
    /// visible once the timeline has finished).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
}

impl Timing {
    pub fn is_on_screen(&self, time_ms: f64) -> bool {
        let start = self.start_ms as f64;
        match self.duration_ms {
            None => time_ms >= start,
            Some(d) => time_ms >= start && time_ms < start + d as f64,
        }
    }
}

/// A named look for an object. Only the fields that are `Some` differ from
/// the object's Normal appearance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectState {
    pub name: String,
    #[serde(default)]
    pub overrides: StateOverrides,
}

impl ObjectState {
    pub fn new(name: impl Into<String>) -> Self {
        ObjectState { name: name.into(), overrides: StateOverrides::default() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct StateOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_windows() {
        let open = Timing { start_ms: 1000, duration_ms: None };
        assert!(!open.is_on_screen(999.0));
        assert!(open.is_on_screen(1000.0));
        assert!(open.is_on_screen(1e9));

        let closed = Timing { start_ms: 1000, duration_ms: Some(500) };
        assert!(closed.is_on_screen(1499.0));
        assert!(!closed.is_on_screen(1500.0));
    }
}
