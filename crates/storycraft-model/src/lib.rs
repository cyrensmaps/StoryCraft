//! The StoryCraft document model.
//!
//! A [`Project`] is a tree of scenes → slides → layers → objects. Every slide
//! has a base layer plus any number of additional layers that triggers can
//! show and hide. Behaviour is expressed with [`Trigger`]s, which pair an
//! [`Action`] with the [`Event`] that fires it and optional [`Condition`]s.
//!
//! The model is plain data: it has no runtime behaviour of its own. The
//! `storycraft-engine` crate interprets it.

#[cfg(feature = "archive")]
pub mod archive;
mod geometry;
mod id;
mod object;
mod project;
pub mod sample;
mod trigger;
mod variable;

pub use geometry::{Color, Point, Rect, Size};
pub use id::Id;
pub use object::{
    HAlign, Object, ObjectKind, ObjectState, ShapeKind, StateOverrides, Stroke, Style, TextContent, Timing, VAlign,
    WebDisplay, WebSource, state_names,
};
pub use project::{
    Advance, Asset, Completion, FORMAT_VERSION, Layer, LmsSettings, PlayerSettings, Project, Scene, Slide,
    SlideLocation,
};
pub use trigger::{
    Action, Comparison, Condition, ConditionClause, Event, Join, LayerTarget, Operand, SlideTarget, Trigger, VarOp,
};
pub use variable::{Value, ValueKind, Variable};
