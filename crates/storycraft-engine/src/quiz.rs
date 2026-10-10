//! Quiz rules: answer state, grading and results (see `docs/quizzes.md`).

use serde::{Deserialize, Serialize};
use storycraft_model::{Id, Project, Question, QuestionKind};

/// The learner's answer to one question slide.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    /// Selected choice objects (choice-based questions).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected: Vec<Id>,
    /// Typed answer (fill in the blank).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// For each matching item, the index of the answer paired with it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<Option<usize>>,
    /// Submissions used so far.
    #[serde(default)]
    pub attempts: u32,
    /// Set once the question is final: correct, or out of attempts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<bool>,
}

impl Answer {
    pub fn is_locked(&self) -> bool {
        self.result.is_some()
    }
}

/// Grade an answer: `None` when nothing has been answered yet.
pub fn grade(question: &Question, answer: &Answer) -> Option<bool> {
    let correct_choices = || question.choices().iter().filter(|c| c.correct).map(|c| &c.object);
    match &question.kind {
        QuestionKind::MultipleChoice { .. } | QuestionKind::TrueFalse { .. } => {
            let chosen = answer.selected.first()?;
            Some(answer.selected.len() == 1 && correct_choices().any(|c| c == chosen))
        }
        QuestionKind::MultipleResponse { .. } => {
            if answer.selected.is_empty() {
                return None;
            }
            let mut wanted: Vec<&Id> = correct_choices().collect();
            let mut given: Vec<&Id> = answer.selected.iter().collect();
            wanted.sort();
            wanted.dedup();
            given.sort();
            given.dedup();
            Some(wanted == given)
        }
        QuestionKind::FillIn { answers, case_sensitive } => {
            let typed = answer.text.trim();
            if typed.is_empty() {
                return None;
            }
            let matches = |accepted: &String| {
                let accepted = accepted.trim();
                if *case_sensitive { accepted == typed } else { accepted.to_lowercase() == typed.to_lowercase() }
            };
            Some(answers.iter().filter(|a| !a.trim().is_empty()).any(matches))
        }
        QuestionKind::Matching { pairs } => {
            if answer.matches.iter().all(Option::is_none) {
                return None;
            }
            Some(pairs.iter().enumerate().all(|(i, _)| answer.matches.get(i).copied().flatten() == Some(i)))
        }
    }
}

/// The quiz score across every question slide.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuizResults {
    pub points: f32,
    pub max_points: f32,
    /// 0–100, rounded.
    pub percent: u32,
    pub passing_percent: f32,
    pub passed: bool,
    pub questions: usize,
    /// Questions that are final (correct or out of attempts).
    pub answered: usize,
}

pub fn results(project: &Project, answer_for: impl Fn(&Id) -> Option<Answer>) -> QuizResults {
    let mut points = 0.0;
    let mut max_points = 0.0;
    let mut questions = 0;
    let mut answered = 0;
    for (_, slide) in project.slides() {
        let Some(question) = &slide.question else { continue };
        questions += 1;
        max_points += question.points.max(0.0);
        if let Some(result) = answer_for(&slide.id).and_then(|a| a.result) {
            answered += 1;
            if result {
                points += question.points.max(0.0);
            }
        }
    }
    let percent = if max_points > 0.0 { (points / max_points * 100.0).round() as u32 } else { 0 };
    let passing_percent = project.quiz.passing_percent;
    QuizResults {
        points,
        max_points,
        percent,
        passing_percent,
        passed: questions > 0 && percent as f32 >= passing_percent,
        questions,
        answered,
    }
}
