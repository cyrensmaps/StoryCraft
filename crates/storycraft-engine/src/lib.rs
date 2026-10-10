//! The StoryCraft runtime.
//!
//! [`Engine`] plays a [`Project`]: it tracks the current slide, which layers
//! are visible, timeline positions, variable values and object states, and
//! runs triggers in response to input and time. It is deliberately free of
//! any rendering or platform code. Callers feed it pointer input, key presses
//! and elapsed time, and draw the [`Frame`] it produces. The editor preview
//! and the published WebAssembly player both drive this same type, so
//! behaviour stays identical in both.
//!
//! # Semantics
//!
//! * Triggers on a layer only listen while that layer is visible. Matching
//!   triggers run in order: base layer first, then shown layers bottom to top,
//!   and within a layer in list order.
//! * Conditions are checked when each trigger runs, not when the event fires.
//! * Jumping to another slide stops any remaining triggers for that event.
//! * Changing a variable queues a "variable changes" event, which is handled
//!   after the current event's triggers finish (breadth-first). Runaway
//!   cascades are cut off after [`MAX_CASCADE`] events.
//! * Object states persist for the whole session, so revisiting a slide
//!   shows the states it was left in.

mod quiz;
mod render;
mod text;

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use storycraft_model::{
    Action, Advance, Comparison, Completion, Condition, ConditionClause, Event, Id, Join, Layer, LayerTarget, Object,
    Operand, Point, Project, QuestionKind, Slide, SlideLocation, SlideTarget, Trigger, Value, ValueKind, VarOp,
    state_names,
};

pub use quiz::{Answer, QuizResults};
pub use render::{Frame, RenderObject, RenderText};
pub use text::substitute_variables;

/// Upper bound on events handled in response to a single input, so a
/// variable-change loop cannot hang the player.
pub const MAX_CASCADE: usize = 10_000;

/// Something the host needs to do on the engine's behalf.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Effect {
    OpenUrl(String),
    /// Report completion to the LMS. Emitted at most once per session.
    Completed,
    /// The learner asked to leave the course.
    Exit,
    /// A new slide is showing (useful for LMS bookmarking).
    SlideChanged(Id),
    /// A message for the learner, e.g. "answer the question first".
    Notice(String),
    /// Quiz results were submitted: report them to the LMS.
    QuizSubmitted(QuizResults),
}

/// Saved learner progress, for LMS suspend data and resume.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub slide: Option<Id>,
    #[serde(default)]
    pub variables: Vec<(Id, Value)>,
    #[serde(default)]
    pub states: Vec<(Id, String)>,
    #[serde(default)]
    pub visited: Vec<Id>,
    #[serde(default)]
    pub completed: bool,
    /// Quiz answers by question slide id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<(Id, Answer)>,
}

/// Timeline state of one visible layer.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerRun {
    pub layer: Id,
    pub time_ms: f64,
    /// Paused by a trigger (as opposed to by a layer that pauses the base).
    pub paused: bool,
    pub ended: bool,
}

impl LayerRun {
    fn new(layer: Id) -> Self {
        LayerRun { layer, time_ms: 0.0, paused: false, ended: false }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Pending {
    TimelineStarts(Id),
    TimelineEnds(Id),
    /// An event that any visible layer may listen for.
    Broadcast(Event),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    NoSlides,
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::NoSlides => f.write_str("the project has no slides"),
        }
    }
}

impl std::error::Error for EngineError {}

pub struct Engine {
    project: Project,
    order: Vec<SlideLocation>,
    vars: HashMap<Id, Value>,
    states: HashMap<Id, String>,
    visited: HashSet<Id>,
    /// Quiz answers by question slide id.
    answers: HashMap<Id, Answer>,
    current: Option<usize>,
    history: Vec<usize>,
    /// `layers[0]` is the base layer; the rest are shown layers in show order.
    layers: Vec<LayerRun>,
    hovered: Option<Id>,
    pressed: Option<Id>,
    completed: bool,
    effects: Vec<Effect>,
    queue: VecDeque<Pending>,
    /// Bumped on every slide change so in-flight trigger lists can stop.
    slide_generation: u64,
}

impl Engine {
    pub fn new(project: Project) -> Result<Self, EngineError> {
        let order: Vec<_> = project.slides().map(|(loc, _)| loc).collect();
        if order.is_empty() {
            return Err(EngineError::NoSlides);
        }
        let vars = project.variables.iter().map(|v| (v.id.clone(), v.value.clone())).collect();
        Ok(Engine {
            project,
            order,
            vars,
            states: HashMap::new(),
            visited: HashSet::new(),
            answers: HashMap::new(),
            current: None,
            history: Vec::new(),
            layers: Vec::new(),
            hovered: None,
            pressed: None,
            completed: false,
            effects: Vec::new(),
            queue: VecDeque::new(),
            slide_generation: 0,
        })
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Show the start slide, or the slide saved by [`Engine::restore`].
    pub fn start(&mut self) {
        let resume = self.current.take();
        let index = resume
            .or_else(|| {
                let id = self.project.start_slide_id()?;
                self.order.iter().position(|loc| &self.slide_at(*loc).id == id)
            })
            .unwrap_or(0);
        self.enter_slide(index);
        self.pump();
    }

    /// Restore saved progress. Call before [`Engine::start`]. Entries that no
    /// longer exist in the project are ignored.
    pub fn restore(&mut self, snapshot: &Snapshot) {
        for (id, value) in &snapshot.variables {
            if let Some(var) = self.project.variable(id) {
                self.vars.insert(id.clone(), value.coerce(var.value.kind()));
            }
        }
        for (id, state) in &snapshot.states {
            if self.project.find_object(id).is_some() {
                self.states.insert(id.clone(), state.clone());
            }
        }
        self.visited = snapshot.visited.iter().filter(|id| self.project.slide(id).is_some()).cloned().collect();
        self.completed = snapshot.completed;
        self.answers = snapshot
            .answers
            .iter()
            .filter(|(id, _)| self.project.slide(id).is_some_and(|s| s.question.is_some()))
            .cloned()
            .collect();
        self.current =
            snapshot.slide.as_ref().and_then(|id| self.order.iter().position(|loc| &self.slide_at(*loc).id == id));
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut variables: Vec<_> = self.vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        variables.sort_by(|a, b| a.0.cmp(&b.0));
        let mut states: Vec<_> = self.states.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        states.sort();
        let mut visited: Vec<_> = self.visited.iter().cloned().collect();
        visited.sort();
        let mut answers: Vec<_> = self.answers.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        answers.sort_by(|a, b| a.0.cmp(&b.0));
        Snapshot {
            slide: self.current_slide().map(|s| s.id.clone()),
            variables,
            states,
            visited,
            completed: self.completed,
            answers,
        }
    }

    // --- quiz --------------------------------------------------------------

    /// The learner's answer on a question slide, if they have started it.
    pub fn answer(&self, slide: &Id) -> Option<&Answer> {
        self.answers.get(slide)
    }

    pub fn quiz_results(&self) -> QuizResults {
        quiz::results(&self.project, |id| self.answers.get(id).cloned())
    }

    /// `%Results.…%` values for text.
    pub fn results_variable(&self, name: &str) -> Option<String> {
        let r = self.quiz_results();
        let number = |n: f32| Value::Number(n as f64).to_string();
        Some(match name {
            "Results.ScorePoints" => number(r.points),
            "Results.MaxPoints" => number(r.max_points),
            "Results.ScorePercent" => r.percent.to_string(),
            "Results.PassPercent" => number(r.passing_percent),
            _ => return None,
        })
    }

    /// Type an answer into the current fill-in-the-blank question.
    pub fn set_text_answer(&mut self, text: &str) {
        if let Some(answer) = self.open_answer(|k| matches!(k, QuestionKind::FillIn { .. })) {
            answer.text = text.to_owned();
        }
    }

    /// Pair matching item `item` with answer `answer` (or clear it with `None`)
    /// on the current matching question.
    pub fn set_match(&mut self, item: usize, answer: Option<usize>) {
        let Some(pairs) = self.current_slide().and_then(|s| match s.question.as_ref().map(|q| &q.kind) {
            Some(QuestionKind::Matching { pairs }) => Some(pairs.len()),
            _ => None,
        }) else {
            return;
        };
        if item >= pairs || answer.is_some_and(|a| a >= pairs) {
            return;
        }
        if let Some(state) = self.open_answer(|k| matches!(k, QuestionKind::Matching { .. })) {
            state.matches.resize(pairs, None);
            state.matches[item] = answer;
        }
    }

    /// The current slide's answer, if its question is of the wanted kind and
    /// not yet final.
    fn open_answer(&mut self, kind: impl Fn(&QuestionKind) -> bool) -> Option<&mut Answer> {
        let slide = self.current_slide()?;
        slide.question.as_ref().filter(|q| kind(&q.kind))?;
        let id = slide.id.clone();
        Some(self.answers.entry(id).or_default()).filter(|a| !a.is_locked())
    }

    /// A click on an answer choice: select it (and, for single-answer
    /// questions, deselect the others). Returns false if `object` is not a
    /// choice on the current question.
    fn select_choice(&mut self, object: &Id) -> bool {
        let Some(slide) = self.current_slide() else { return false };
        let Some(question) = slide.question.clone() else { return false };
        if !question.choices().iter().any(|c| &c.object == object) {
            return false;
        }
        let answer = self.answers.entry(slide.id.clone()).or_default();
        if answer.is_locked() {
            return true;
        }
        if matches!(question.kind, QuestionKind::MultipleResponse { .. }) {
            if let Some(i) = answer.selected.iter().position(|s| s == object) {
                answer.selected.remove(i);
            } else {
                answer.selected.push(object.clone());
            }
        } else {
            answer.selected = vec![object.clone()];
        }
        let selected = answer.selected.clone();
        for choice in question.choices() {
            let state = if selected.contains(&choice.object) { state_names::SELECTED } else { state_names::NORMAL };
            self.states.insert(choice.object.clone(), state.to_owned());
        }
        true
    }

    fn submit_interaction(&mut self) {
        let Some(slide) = self.current_slide() else { return };
        let Some(question) = slide.question.clone() else { return };
        let answer = self.answers.entry(slide.id.clone()).or_default();
        if answer.is_locked() {
            return;
        }
        let Some(correct) = quiz::grade(&question, answer) else {
            self.effects.push(Effect::Notice("Please answer the question before submitting.".into()));
            return;
        };
        answer.attempts += 1;
        let out_of_attempts = question.attempts != 0 && answer.attempts >= question.attempts;
        if correct || out_of_attempts {
            answer.result = Some(correct);
        }
        let feedback = &question.feedback;
        let layer = if correct {
            feedback.correct.clone()
        } else if out_of_attempts {
            feedback.incorrect.clone()
        } else {
            feedback.try_again.clone().or_else(|| feedback.incorrect.clone())
        };
        if let Some(layer) = layer {
            self.show_layer(&layer);
        }
    }

    fn submit_results(&mut self) {
        let results = self.quiz_results();
        self.effects.push(Effect::QuizSubmitted(results));
        if self.project.lms.completion == Completion::QuizSubmitted {
            self.complete();
        }
    }

    fn reset_results(&mut self) {
        let choices: Vec<Id> = self
            .project
            .slides()
            .filter_map(|(_, s)| s.question.as_ref())
            .flat_map(|q| q.choices().iter().map(|c| c.object.clone()))
            .collect();
        for object in choices {
            self.states.insert(object, state_names::NORMAL.to_owned());
        }
        self.answers.clear();
    }

    // --- queries -----------------------------------------------------------

    pub fn current_slide(&self) -> Option<&Slide> {
        self.current.map(|i| self.slide_at(self.order[i]))
    }

    /// 1-based position of the current slide in the course, and the total.
    pub fn position(&self) -> (usize, usize) {
        (self.current.map_or(0, |i| i + 1), self.order.len())
    }

    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        self.current.is_some_and(|i| i + 1 < self.order.len())
    }

    pub fn variable(&self, id: &Id) -> Option<&Value> {
        self.vars.get(id)
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }

    pub fn layer_runs(&self) -> &[LayerRun] {
        &self.layers
    }

    pub fn is_layer_visible(&self, id: &Id) -> bool {
        self.layers.iter().any(|l| &l.layer == id)
    }

    /// The object's state ignoring pointer feedback.
    pub fn object_state<'a>(&'a self, object: &'a Object) -> &'a str {
        match self.states.get(&object.id) {
            Some(s) => s,
            None if object.initially_hidden => state_names::HIDDEN,
            None => state_names::NORMAL,
        }
    }

    /// The state to draw, including automatic Hover and Down.
    pub fn display_state<'a>(&'a self, object: &'a Object) -> &'a str {
        let base = self.object_state(object);
        let feedback_allowed = matches!(base, state_names::NORMAL | state_names::VISITED | state_names::SELECTED);
        if feedback_allowed {
            if self.pressed.as_ref() == Some(&object.id) && object.state(state_names::DOWN).is_some() {
                return state_names::DOWN;
            }
            if self.hovered.as_ref() == Some(&object.id) && object.state(state_names::HOVER).is_some() {
                return state_names::HOVER;
            }
        }
        base
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    /// Everything needed to draw the current moment.
    pub fn frame(&self) -> Frame {
        render::frame_for_engine(self)
    }

    /// Objects on screen in draw order, with the time of their layer.
    fn visible_objects(&self) -> impl Iterator<Item = (&Layer, &Object)> {
        self.visible_layers_in_order().flat_map(move |(layer, time)| {
            layer
                .objects
                .iter()
                .filter(move |o| o.timing.is_on_screen(time) && self.object_state(o) != state_names::HIDDEN)
                .map(move |o| (layer, o))
        })
    }

    /// Visible layers, bottom to top, paired with their timeline position.
    fn visible_layers_in_order(&self) -> impl Iterator<Item = (&Layer, f64)> {
        let slide = self.current_slide();
        self.layers.iter().filter_map(move |run| {
            let layer = slide?.layer(&run.layer)?;
            Some((layer, run.time_ms))
        })
    }

    /// Whether clicking or hovering `object` can do anything.
    fn is_interactive(&self, object: &Object) -> bool {
        object.has_pointer_feedback()
            || object.state(state_names::SELECTED).is_some()
            || object.state(state_names::VISITED).is_some()
            || self.visible_layers_in_order().any(|(layer, _)| {
                layer.triggers.iter().any(|t| {
                    t.enabled
                        && matches!(&t.event,
                            Event::Click { object: o } | Event::HoverStart { object: o } | Event::HoverEnd { object: o }
                            if o == &object.id)
                })
            })
    }

    /// The topmost interactive, enabled object under `p` (slide coordinates).
    pub fn hit_test(&self, p: Point) -> Option<&Object> {
        let objects: Vec<_> = self.reachable_objects().collect();
        objects
            .into_iter()
            .rev()
            .find(|o| self.object_state(o) != state_names::DISABLED && self.is_interactive(o) && contains_rotated(o, p))
    }

    /// Visible objects the learner can interact with: everything on screen
    /// except base-layer objects while a layer that blocks them is shown.
    fn reachable_objects(&self) -> impl Iterator<Item = &Object> {
        let base_blocked = self.base_blocked();
        let base = self.current_slide().map(|s| s.base.id.clone());
        self.visible_objects().filter(move |(l, _)| !(base_blocked && Some(&l.id) == base.as_ref())).map(|(_, o)| o)
    }

    /// Whether a visible layer prevents clicks on the base layer.
    fn base_blocked(&self) -> bool {
        let Some(slide) = self.current_slide() else { return false };
        self.layers.iter().skip(1).any(|run| slide.layer(&run.layer).is_some_and(|l| l.prevent_base_clicks))
    }

    // --- input -------------------------------------------------------------

    /// Advance all playing timelines by `dt_ms` milliseconds.
    pub fn tick(&mut self, dt_ms: f64) {
        if self.current.is_none() || dt_ms.is_nan() || dt_ms <= 0.0 {
            return;
        }
        let base_held = self.base_held();
        let Some(slide) = self.current_slide() else { return };
        let durations: Vec<f64> =
            self.layers.iter().map(|run| slide.layer(&run.layer).map_or(0.0, |l| l.duration_ms as f64)).collect();
        let mut ended = Vec::new();
        for (i, run) in self.layers.iter_mut().enumerate() {
            if run.ended || run.paused || (i == 0 && base_held) {
                continue;
            }
            run.time_ms = (run.time_ms + dt_ms).min(durations[i]);
            if run.time_ms >= durations[i] {
                run.ended = true;
                ended.push(run.layer.clone());
            }
        }
        for layer in ended {
            self.queue.push_back(Pending::TimelineEnds(layer));
        }
        self.pump();
    }

    /// The pointer moved to `p` (slide coordinates), or left the slide (`None`).
    pub fn pointer_move(&mut self, p: Option<Point>) {
        let now = p.and_then(|p| self.hit_test(p)).map(|o| o.id.clone());
        if now == self.hovered {
            return;
        }
        if let Some(old) = self.hovered.take() {
            self.queue.push_back(Pending::Broadcast(Event::HoverEnd { object: old }));
        }
        if let Some(new) = &now {
            self.queue.push_back(Pending::Broadcast(Event::HoverStart { object: new.clone() }));
        }
        self.hovered = now;
        self.pump();
    }

    pub fn pointer_down(&mut self, p: Point) {
        self.pressed = self.hit_test(p).map(|o| o.id.clone());
    }

    /// Release the pointer; a click happens if it is released over the object
    /// it was pressed on.
    pub fn pointer_up(&mut self, p: Point) {
        let pressed = self.pressed.take();
        let under = self.hit_test(p).map(|o| o.id.clone());
        if let Some(id) = pressed.filter(|id| Some(id) == under.as_ref()) {
            self.click(&id);
        }
    }

    /// Click an object directly (keyboard activation, tests). Ignored unless
    /// the object is on screen, enabled and not blocked by a layer.
    pub fn click(&mut self, id: &Id) {
        let Some(object) = self.reachable_objects().find(|o| &o.id == id) else {
            return;
        };
        let state = self.object_state(object).to_owned();
        if state == state_names::DISABLED {
            return;
        }
        let has_selected = object.state(state_names::SELECTED).is_some();
        let has_visited = object.state(state_names::VISITED).is_some();
        // Answer choices select through the question; other objects use the
        // built-in state behaviour. Either runs before the object's own
        // triggers so that an explicit "change state" trigger has the final say.
        if self.select_choice(id) {
            self.queue.push_back(Pending::Broadcast(Event::Click { object: id.clone() }));
            self.pump();
            return;
        }
        let new_state = if has_selected {
            Some(if state == state_names::SELECTED { state_names::NORMAL } else { state_names::SELECTED })
        } else if has_visited && state == state_names::NORMAL {
            Some(state_names::VISITED)
        } else {
            None
        };
        if let Some(s) = new_state {
            self.states.insert(id.clone(), s.to_owned());
        }
        self.queue.push_back(Pending::Broadcast(Event::Click { object: id.clone() }));
        self.pump();
    }

    /// A key was pressed. `key` uses DOM `KeyboardEvent.key` names.
    pub fn key_pressed(&mut self, key: &str) {
        self.queue.push_back(Pending::Broadcast(Event::KeyPressed { key: key.to_owned() }));
        self.pump();
    }

    /// The player's own Next button.
    pub fn next_slide(&mut self) {
        self.jump(&SlideTarget::Next);
        self.pump();
    }

    /// The player's own Previous button.
    pub fn previous_slide(&mut self) {
        self.jump(&SlideTarget::Previous);
        self.pump();
    }

    /// Jump straight to a slide (editor "preview from here", menus).
    pub fn go_to_slide(&mut self, id: &Id) {
        self.jump(&SlideTarget::Slide(id.clone()));
        self.pump();
    }

    // --- internals ---------------------------------------------------------

    fn slide_at(&self, loc: SlideLocation) -> &Slide {
        self.project.slide_at(loc).expect("slide order is built from the project")
    }

    /// Whether a visible layer is holding the base timeline paused.
    fn base_held(&self) -> bool {
        let Some(slide) = self.current_slide() else { return false };
        self.layers.iter().skip(1).any(|run| slide.layer(&run.layer).is_some_and(|l| l.pause_base_timeline))
    }

    fn enter_slide(&mut self, index: usize) {
        self.current = Some(index);
        self.slide_generation += 1;
        self.queue.clear();
        self.hovered = None;
        self.pressed = None;
        let slide = self.slide_at(self.order[index]);
        let (slide_id, base_id) = (slide.id.clone(), slide.base.id.clone());
        self.layers = vec![LayerRun::new(base_id.clone())];
        self.visited.insert(slide_id.clone());
        self.effects.push(Effect::SlideChanged(slide_id));
        self.check_completion();
        self.queue.push_back(Pending::TimelineStarts(base_id));
    }

    fn check_completion(&mut self) {
        if self.completed {
            return;
        }
        let done = match self.project.lms.completion {
            Completion::AllSlidesViewed => self.visited.len() >= self.order.len(),
            Completion::SlidesViewed { count } => self.visited.len() >= count as usize,
            Completion::OnTrigger | Completion::QuizSubmitted => false,
        };
        if done {
            self.complete();
        }
    }

    fn complete(&mut self) {
        if !self.completed {
            self.completed = true;
            self.effects.push(Effect::Completed);
        }
    }

    fn pump(&mut self) {
        let mut budget = MAX_CASCADE;
        while let Some(pending) = self.queue.pop_front() {
            if budget == 0 {
                self.queue.clear();
                break;
            }
            budget -= 1;
            self.handle(pending);
        }
    }

    fn handle(&mut self, pending: Pending) {
        let Some(slide) = self.current_slide() else { return };
        let generation = self.slide_generation;

        let matches = |layer: &Layer, t: &Trigger| -> bool {
            t.enabled
                && match &pending {
                    Pending::TimelineStarts(id) => &layer.id == id && t.event == Event::TimelineStarts,
                    Pending::TimelineEnds(id) => &layer.id == id && t.event == Event::TimelineEnds,
                    Pending::Broadcast(Event::KeyPressed { key }) => match &t.event {
                        Event::KeyPressed { key: k } => keys_match(k, key),
                        _ => false,
                    },
                    Pending::Broadcast(event) => &t.event == event,
                }
        };
        let triggers: Vec<(Id, Trigger)> = self
            .visible_layers_in_order()
            .flat_map(|(layer, _)| {
                layer.triggers.iter().filter(|t| matches(layer, t)).map(|t| (layer.id.clone(), t.clone()))
            })
            .collect();
        let base_ended = matches!(&pending, Pending::TimelineEnds(id) if id == &slide.base.id);
        let auto_advance = slide.advance == Advance::Automatically;
        let auto_hide = match &pending {
            Pending::TimelineEnds(id) => slide.layers.iter().find(|l| &l.id == id && l.hide_when_timeline_ends),
            _ => None,
        }
        .map(|l| l.id.clone());

        for (layer, trigger) in triggers {
            if self.slide_generation != generation {
                return;
            }
            if !self.is_layer_visible(&layer) || !self.conditions_hold(&trigger.conditions) {
                continue;
            }
            self.execute(&layer, &trigger.action);
        }
        // Built-in behaviour runs after the layer's own "timeline ends" triggers.
        if let Some(layer) = auto_hide.filter(|_| self.slide_generation == generation) {
            self.hide_layer(&layer);
        }
        if base_ended && auto_advance && self.slide_generation == generation {
            self.jump(&SlideTarget::Next);
        }
    }

    fn conditions_hold(&self, clauses: &[ConditionClause]) -> bool {
        // OR-separated groups of AND-ed clauses.
        let mut any_group = false;
        let mut group = true;
        for (i, clause) in clauses.iter().enumerate() {
            if i > 0 && clause.join == Join::Or {
                any_group |= group;
                group = true;
            }
            group = group && self.condition_holds(&clause.condition);
        }
        any_group || group
    }

    fn condition_holds(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Variable { variable, cmp, value } => {
                let Some(left) = self.vars.get(variable) else { return false };
                let Some(right) = self.operand(value) else { return false };
                compare(left, *cmp, &right)
            }
            Condition::ObjectState { object, state, negate } => {
                let current = self
                    .project
                    .find_object(object)
                    .map(|(_, _, o)| self.object_state(o))
                    .unwrap_or(state_names::NORMAL);
                (current == state) != *negate
            }
            Condition::QuizPassed { negate } => self.quiz_results().passed != *negate,
        }
    }

    fn operand(&self, operand: &Operand) -> Option<Value> {
        match operand {
            Operand::Literal(v) => Some(v.clone()),
            Operand::Variable(id) => self.vars.get(id).cloned(),
        }
    }

    fn resolve_layer(&self, owner: &Id, target: &LayerTarget) -> Id {
        match target {
            LayerTarget::This => owner.clone(),
            LayerTarget::Layer(id) => id.clone(),
        }
    }

    fn execute(&mut self, owner: &Id, action: &Action) {
        match action {
            Action::JumpToSlide { target } => self.jump(target),
            Action::ShowLayer { layer } => self.show_layer(layer),
            Action::HideLayer { layer } => {
                let id = self.resolve_layer(owner, layer);
                self.hide_layer(&id);
            }
            Action::ChangeState { object, state } => {
                if let Some((_, _, o)) = self.project.find_object(object) {
                    if o.has_state(state) {
                        self.states.insert(object.clone(), state.clone());
                    }
                }
            }
            Action::SetVariable { variable, op, value } => self.set_variable(variable, *op, value),
            Action::PauseTimeline { layer } => {
                let id = self.resolve_layer(owner, layer);
                if let Some(run) = self.layers.iter_mut().find(|r| r.layer == id) {
                    run.paused = true;
                }
            }
            Action::ResumeTimeline { layer } => {
                let id = self.resolve_layer(owner, layer);
                if let Some(run) = self.layers.iter_mut().find(|r| r.layer == id) {
                    run.paused = false;
                }
            }
            Action::RestartTimeline { layer } => {
                let id = self.resolve_layer(owner, layer);
                if let Some(run) = self.layers.iter_mut().find(|r| r.layer == id) {
                    *run = LayerRun::new(id.clone());
                    self.queue.push_back(Pending::TimelineStarts(id));
                }
            }
            Action::OpenUrl { url } => self.effects.push(Effect::OpenUrl(url.clone())),
            Action::CompleteCourse => self.complete(),
            Action::ExitCourse => self.effects.push(Effect::Exit),
            Action::SubmitInteraction => self.submit_interaction(),
            Action::SubmitResults => self.submit_results(),
            Action::ResetResults => self.reset_results(),
        }
    }

    fn jump(&mut self, target: &SlideTarget) {
        let Some(current) = self.current else { return };
        let index = match target {
            SlideTarget::Next => Some(current + 1).filter(|i| *i < self.order.len()),
            SlideTarget::Previous => {
                // Going back pops history instead of pushing to it.
                if let Some(prev) = self.history.pop() {
                    self.enter_slide(prev);
                }
                return;
            }
            SlideTarget::Slide(id) => self.order.iter().position(|loc| &self.slide_at(*loc).id == id),
        };
        if let Some(index) = index {
            self.history.push(current);
            self.enter_slide(index);
        }
    }

    fn show_layer(&mut self, id: &Id) {
        let Some(slide) = self.current_slide() else { return };
        let Some(layer) = slide.layers.iter().find(|l| &l.id == id) else { return };
        if layer.hide_other_layers {
            self.layers.truncate(1);
        } else {
            self.layers.retain(|r| &r.layer != id);
        }
        self.layers.push(LayerRun::new(id.clone()));
        self.queue.push_back(Pending::TimelineStarts(id.clone()));
    }

    fn hide_layer(&mut self, id: &Id) {
        // The base layer cannot be hidden.
        if let Some(pos) = self.layers.iter().skip(1).position(|r| &r.layer == id) {
            self.layers.remove(pos + 1);
        }
    }

    fn set_variable(&mut self, id: &Id, op: VarOp, operand: &Operand) {
        let (Some(old), Some(rhs)) = (self.vars.get(id).cloned(), self.operand(operand)) else {
            return;
        };
        let new = apply_op(&old, op, &rhs);
        if let Some(new) = new.filter(|n| n != &old) {
            self.vars.insert(id.clone(), new);
            self.queue.push_back(Pending::Broadcast(Event::VariableChanges { variable: id.clone() }));
        }
    }
}

fn apply_op(old: &Value, op: VarOp, rhs: &Value) -> Option<Value> {
    let kind = old.kind();
    let num = |f: fn(f64, f64) -> f64| -> Option<Value> {
        let (a, b) = (old.as_number()?, rhs.as_number()?);
        let r = f(a, b);
        r.is_finite().then_some(Value::Number(r))
    };
    match (op, kind) {
        (VarOp::Assign, _) => Some(rhs.coerce(kind)),
        (VarOp::Add, ValueKind::Text) => Some(Value::Text(format!("{old}{rhs}"))),
        (VarOp::Add, ValueKind::Number) => num(|a, b| a + b),
        (VarOp::Subtract, ValueKind::Number) => num(|a, b| a - b),
        (VarOp::Multiply, ValueKind::Number) => num(|a, b| a * b),
        (VarOp::Divide, ValueKind::Number) => num(|a, b| a / b),
        (VarOp::Toggle, ValueKind::Bool) => Some(Value::Bool(!old.as_bool())),
        _ => None,
    }
}

fn compare(left: &Value, cmp: Comparison, right: &Value) -> bool {
    use std::cmp::Ordering;

    let (ls, rs) = (left.to_string(), right.to_string());
    match cmp {
        Comparison::Contains => return ls.contains(&rs),
        Comparison::NotContains => return !ls.contains(&rs),
        Comparison::EqIgnoreCase => return ls.to_lowercase() == rs.to_lowercase(),
        _ => {}
    }
    let numeric = left.kind() == ValueKind::Number || right.kind() == ValueKind::Number;
    let ordering = match (left.as_number(), right.as_number()) {
        (Some(a), Some(b)) if numeric => a.partial_cmp(&b),
        _ => Some(ls.cmp(&rs)),
    };
    let Some(ordering) = ordering else { return false };
    match cmp {
        Comparison::Eq => ordering == Ordering::Equal,
        Comparison::Ne => ordering != Ordering::Equal,
        Comparison::Lt => ordering == Ordering::Less,
        Comparison::Le => ordering != Ordering::Greater,
        Comparison::Gt => ordering == Ordering::Greater,
        Comparison::Ge => ordering != Ordering::Less,
        Comparison::EqIgnoreCase | Comparison::Contains | Comparison::NotContains => unreachable!(),
    }
}

/// Single printable characters match case-insensitively ("a" fires on "A");
/// named keys must match exactly.
fn keys_match(wanted: &str, pressed: &str) -> bool {
    if wanted.chars().count() == 1 { wanted.to_lowercase() == pressed.to_lowercase() } else { wanted == pressed }
}

/// Point-in-frame test that honours the object's rotation.
fn contains_rotated(object: &Object, p: Point) -> bool {
    let c = object.frame.center();
    let (sin, cos) = (-object.rotation.to_radians()).sin_cos();
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    let local = Point { x: c.x + dx * cos - dy * sin, y: c.y + dx * sin + dy * cos };
    object.frame.contains(local)
}

#[cfg(test)]
mod quiz_tests;
#[cfg(test)]
mod tests;
