//! Quiz questions. See `docs/quizzes.md` for the rules the engine applies.

use serde::{Deserialize, Serialize};

use crate::Id;

/// Makes a slide a question slide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub kind: QuestionKind,
    /// Points for a correct answer.
    #[serde(default = "default_points")]
    pub points: f32,
    /// Submissions allowed; 0 means unlimited.
    #[serde(default = "default_attempts")]
    pub attempts: u32,
    #[serde(default)]
    pub feedback: Feedback,
}

fn default_points() -> f32 {
    10.0
}

fn default_attempts() -> u32 {
    1
}

impl Question {
    pub fn new(kind: QuestionKind) -> Self {
        Question { kind, points: default_points(), attempts: default_attempts(), feedback: Feedback::default() }
    }

    /// The choices of a choice-based question (multiple choice, true/false,
    /// multiple response); empty for other kinds.
    pub fn choices(&self) -> &[Choice] {
        match &self.kind {
            QuestionKind::MultipleChoice { choices }
            | QuestionKind::TrueFalse { choices }
            | QuestionKind::MultipleResponse { choices } => choices,
            QuestionKind::FillIn { .. } | QuestionKind::Matching { .. } => &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuestionKind {
    /// Exactly one choice is correct; selecting one deselects the others.
    MultipleChoice { choices: Vec<Choice> },
    /// Multiple choice with two choices.
    TrueFalse { choices: Vec<Choice> },
    /// Any number of choices are correct; each choice toggles.
    MultipleResponse { choices: Vec<Choice> },
    /// Typed text compared with the accepted answers.
    FillIn {
        answers: Vec<String>,
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Each item must be paired with its own answer.
    Matching { pairs: Vec<MatchPair> },
}

impl QuestionKind {
    pub fn label(&self) -> &'static str {
        match self {
            QuestionKind::MultipleChoice { .. } => "Multiple choice",
            QuestionKind::TrueFalse { .. } => "True/false",
            QuestionKind::MultipleResponse { .. } => "Multiple response",
            QuestionKind::FillIn { .. } => "Fill in the blank",
            QuestionKind::Matching { .. } => "Matching",
        }
    }
}

/// An answer choice: an object on the question slide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub object: Id,
    #[serde(default)]
    pub correct: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatchPair {
    pub item: String,
    pub answer: String,
}

/// Layers shown after the learner submits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Feedback {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correct: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incorrect: Option<Id>,
    /// Shown after a wrong answer while attempts remain (falls back to `incorrect`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub try_again: Option<Id>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuizSettings {
    /// Percentage needed to pass, 0–100.
    #[serde(default = "default_passing")]
    pub passing_percent: f32,
}

fn default_passing() -> f32 {
    80.0
}

impl Default for QuizSettings {
    fn default() -> Self {
        QuizSettings { passing_percent: default_passing() }
    }
}
