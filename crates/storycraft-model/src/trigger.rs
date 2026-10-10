use serde::{Deserialize, Serialize};

use crate::{Id, Value};

/// "Do `action` when `event` happens, if `conditions` hold."
///
/// Triggers live on a layer and only listen while that layer is visible.
/// Triggers on the base layer act as slide triggers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub id: Id,
    pub action: Action,
    pub event: Event,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<ConditionClause>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

impl Trigger {
    pub fn new(action: Action, event: Event) -> Self {
        Trigger { id: Id::new(), action, event, conditions: Vec::new(), enabled: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The owning layer's timeline starts (slide entered / layer shown).
    TimelineStarts,
    /// The owning layer's timeline reaches its end.
    TimelineEnds,
    /// The learner clicks an object.
    Click { object: Id },
    /// The pointer starts hovering over an object.
    HoverStart { object: Id },
    /// The pointer stops hovering over an object.
    HoverEnd { object: Id },
    /// A variable's value changes.
    VariableChanges { variable: Id },
    /// A key is pressed. Uses DOM `KeyboardEvent.key` names ("Enter", "a", "ArrowLeft").
    KeyPressed { key: String },
}

impl Event {
    pub fn label(&self) -> &'static str {
        match self {
            Event::TimelineStarts => "Timeline starts",
            Event::TimelineEnds => "Timeline ends",
            Event::Click { .. } => "User clicks",
            Event::HoverStart { .. } => "Mouse hovers over",
            Event::HoverEnd { .. } => "Mouse hover ends",
            Event::VariableChanges { .. } => "Variable changes",
            Event::KeyPressed { .. } => "User presses a key",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    JumpToSlide {
        target: SlideTarget,
    },
    ShowLayer {
        layer: Id,
    },
    HideLayer {
        layer: LayerTarget,
    },
    ChangeState {
        object: Id,
        state: String,
    },
    SetVariable {
        variable: Id,
        op: VarOp,
        value: Operand,
    },
    PauseTimeline {
        layer: LayerTarget,
    },
    ResumeTimeline {
        layer: LayerTarget,
    },
    RestartTimeline {
        layer: LayerTarget,
    },
    OpenUrl {
        url: String,
    },
    /// Mark the course complete (and passed) in the LMS.
    CompleteCourse,
    ExitCourse,
}

impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Action::JumpToSlide { .. } => "Jump to slide",
            Action::ShowLayer { .. } => "Show layer",
            Action::HideLayer { .. } => "Hide layer",
            Action::ChangeState { .. } => "Change state of",
            Action::SetVariable { .. } => "Adjust variable",
            Action::PauseTimeline { .. } => "Pause timeline",
            Action::ResumeTimeline { .. } => "Resume timeline",
            Action::RestartTimeline { .. } => "Restart timeline",
            Action::OpenUrl { .. } => "Open URL",
            Action::CompleteCourse => "Complete course",
            Action::ExitCourse => "Exit course",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "slide", rename_all = "snake_case")]
pub enum SlideTarget {
    Next,
    Previous,
    Slide(Id),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "layer", rename_all = "snake_case")]
pub enum LayerTarget {
    /// The layer that owns the trigger.
    This,
    Layer(Id),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarOp {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    /// Flip a True/False variable. The operand is ignored.
    Toggle,
}

impl VarOp {
    pub const ALL: [VarOp; 6] =
        [VarOp::Assign, VarOp::Add, VarOp::Subtract, VarOp::Multiply, VarOp::Divide, VarOp::Toggle];

    pub fn label(&self) -> &'static str {
        match self {
            VarOp::Assign => "= Assignment",
            VarOp::Add => "+ Add",
            VarOp::Subtract => "- Subtract",
            VarOp::Multiply => "* Multiply",
            VarOp::Divide => "/ Divide",
            VarOp::Toggle => "= Not Assignment (toggle)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Operand {
    Literal(Value),
    Variable(Id),
}

/// One clause of a trigger's condition list.
///
/// Clauses combine left to right with the usual precedence: AND binds
/// tighter than OR, so `a OR b AND c` means `a OR (b AND c)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConditionClause {
    /// How this clause joins the previous one. Ignored on the first clause.
    #[serde(default)]
    pub join: Join,
    pub condition: Condition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Join {
    #[default]
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    Variable {
        variable: Id,
        cmp: Comparison,
        value: Operand,
    },
    /// True when the object's current state is (or, with `negate`, is not) `state`.
    ObjectState {
        object: Id,
        state: String,
        #[serde(default)]
        negate: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// Text equality ignoring ASCII case.
    EqIgnoreCase,
    Contains,
    NotContains,
}

impl Comparison {
    pub const ALL: [Comparison; 9] = [
        Comparison::Eq,
        Comparison::Ne,
        Comparison::Lt,
        Comparison::Le,
        Comparison::Gt,
        Comparison::Ge,
        Comparison::EqIgnoreCase,
        Comparison::Contains,
        Comparison::NotContains,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Comparison::Eq => "==",
            Comparison::Ne => "!=",
            Comparison::Lt => "<",
            Comparison::Le => "<=",
            Comparison::Gt => ">",
            Comparison::Ge => ">=",
            Comparison::EqIgnoreCase => "== (ignore case)",
            Comparison::Contains => "contains",
            Comparison::NotContains => "does not contain",
        }
    }
}
