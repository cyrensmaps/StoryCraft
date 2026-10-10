use storycraft_model::*;

use super::*;

/// A five-slide quiz: multiple choice (2 attempts, feedback layers),
/// multiple response, fill in the blank, matching, and a results slide.
struct Quiz {
    project: Project,
    mc: [Id; 3],
    mc_submit: Id,
    mc_layers: [Id; 3],
    mr: [Id; 3],
    submit: [Id; 3],
    results_text: Id,
    passed_flag: Id,
}

fn button(name: &str) -> Object {
    let mut o = Object::new(name, ObjectKind::Button, Rect::new(0.0, 0.0, 100.0, 40.0));
    o.text = Some(TextContent::new(name));
    o
}

fn question_slide(name: &str, kind: QuestionKind, objects: Vec<Object>) -> (Slide, Id) {
    let mut slide = Slide::new(name);
    let submit = button("Submit");
    let submit_id = submit.id.clone();
    slide.base.triggers.push(Trigger::new(Action::SubmitInteraction, Event::Click { object: submit_id.clone() }));
    slide.base.objects = objects;
    slide.base.objects.push(submit);
    slide.question = Some(Question::new(kind));
    (slide, submit_id)
}

fn quiz() -> Quiz {
    let mut p = Project::new("quiz");
    let passed = Variable::new("Passed", Value::Bool(false));
    let passed_flag = passed.id.clone();
    p.variables.push(passed);

    // 1. Multiple choice with feedback layers and two attempts.
    let mc_objects = [button("A"), button("B"), button("C")];
    let mc = mc_objects.clone().map(|o| o.id);
    let choices = vec![
        Choice { object: mc[0].clone(), correct: true },
        Choice { object: mc[1].clone(), correct: false },
        Choice { object: mc[2].clone(), correct: false },
    ];
    let (mut s1, mc_submit) = question_slide("MC", QuestionKind::MultipleChoice { choices }, mc_objects.to_vec());
    let layers = [Layer::new("Correct"), Layer::new("Incorrect"), Layer::new("Try Again")];
    let mc_layers = layers.clone().map(|l| l.id);
    let q = s1.question.as_mut().unwrap();
    q.attempts = 2;
    q.feedback = Feedback {
        correct: Some(mc_layers[0].clone()),
        incorrect: Some(mc_layers[1].clone()),
        try_again: Some(mc_layers[2].clone()),
    };
    s1.layers = layers.to_vec();

    // 2. Multiple response: X and Y are correct.
    let mr_objects = [button("X"), button("Y"), button("Z")];
    let mr = mr_objects.clone().map(|o| o.id);
    let choices = vec![
        Choice { object: mr[0].clone(), correct: true },
        Choice { object: mr[1].clone(), correct: true },
        Choice { object: mr[2].clone(), correct: false },
    ];
    let (s2, mr_submit) = question_slide("MR", QuestionKind::MultipleResponse { choices }, mr_objects.to_vec());

    // 3. Fill in the blank.
    let (s3, fill_submit) = question_slide(
        "Fill",
        QuestionKind::FillIn { answers: vec!["Paris".into(), "paris, france".into()], case_sensitive: false },
        vec![],
    );

    // 4. Matching.
    let pairs = ["Red/Rouge", "Blue/Bleu", "Green/Vert"]
        .iter()
        .map(|p| {
            let (item, answer) = p.split_once('/').unwrap();
            MatchPair { item: item.into(), answer: answer.into() }
        })
        .collect();
    let (s4, match_submit) = question_slide("Match", QuestionKind::Matching { pairs }, vec![]);

    // 5. Results: show the score, submit, and flag a pass.
    let mut s5 = Slide::new("Results");
    let mut text = Object::new("Score", ObjectKind::Text, Rect::new(0.0, 0.0, 300.0, 50.0));
    text.text =
        Some(TextContent::new("You scored %Results.ScorePercent%% (%Results.ScorePoints%/%Results.MaxPoints%)"));
    let results_text = text.id.clone();
    s5.base.objects.push(text);
    s5.base.triggers.push(Trigger::new(Action::SubmitResults, Event::TimelineStarts));
    let mut flag = Trigger::new(
        Action::SetVariable {
            variable: passed_flag.clone(),
            op: VarOp::Assign,
            value: Operand::Literal(Value::Bool(true)),
        },
        Event::TimelineStarts,
    );
    flag.conditions.push(ConditionClause { join: Join::And, condition: Condition::QuizPassed { negate: false } });
    s5.base.triggers.push(flag);

    p.scenes[0].slides = vec![s1, s2, s3, s4, s5];
    p.lms.completion = Completion::QuizSubmitted;
    Quiz {
        project: p,
        mc,
        mc_submit,
        mc_layers,
        mr,
        submit: [mr_submit, fill_submit, match_submit],
        results_text,
        passed_flag,
    }
}

fn started(p: &Project) -> Engine {
    let mut e = Engine::new(p.clone()).unwrap();
    e.start();
    e
}

fn current(e: &Engine) -> Id {
    e.current_slide().unwrap().id.clone()
}

fn shown(e: &Engine, layer: &Id) -> bool {
    e.is_layer_visible(layer)
}

fn state_of(e: &Engine, id: &Id) -> String {
    let (_, _, o) = e.project().find_object(id).unwrap();
    e.object_state(o).to_owned()
}

#[test]
fn project_validates() {
    assert_eq!(quiz().project.validate(), Vec::<String>::new());
}

#[test]
fn multiple_choice_selects_one_and_grades() {
    let q = quiz();
    let mut e = started(&q.project);
    e.click(&q.mc[1]);
    e.click(&q.mc[0]);
    assert_eq!(state_of(&e, &q.mc[0]), state_names::SELECTED);
    assert_eq!(state_of(&e, &q.mc[1]), state_names::NORMAL, "selecting one choice deselects the others");

    e.click(&q.mc_submit);
    assert!(shown(&e, &q.mc_layers[0]), "correct feedback");
    let answer = e.answer(&current(&e)).unwrap();
    assert_eq!((answer.attempts, answer.result), (1, Some(true)));

    e.click(&q.mc[1]);
    assert_eq!(state_of(&e, &q.mc[0]), state_names::SELECTED, "a final question ignores new selections");
}

#[test]
fn wrong_answers_use_attempts_then_lock() {
    let q = quiz();
    let mut e = started(&q.project);
    e.click(&q.mc[2]);
    e.click(&q.mc_submit);
    assert!(shown(&e, &q.mc_layers[2]), "try again while attempts remain");
    assert_eq!(e.answer(&current(&e)).unwrap().result, None);

    e.click(&q.mc[1]);
    e.click(&q.mc_submit);
    assert!(shown(&e, &q.mc_layers[1]), "incorrect once attempts run out");
    let answer = e.answer(&current(&e)).unwrap();
    assert_eq!((answer.attempts, answer.result), (2, Some(false)));
    assert_eq!(e.quiz_results().points, 0.0);
}

#[test]
fn submitting_nothing_is_not_an_attempt() {
    let q = quiz();
    let mut e = started(&q.project);
    e.click(&q.mc_submit);
    assert!(e.take_effects().iter().any(|f| matches!(f, Effect::Notice(_))));
    assert!(e.answer(&current(&e)).is_none_or(|a| a.attempts == 0));
    assert!(!q.mc_layers.iter().any(|l| shown(&e, l)));
}

#[test]
fn multiple_response_needs_exactly_the_correct_set() {
    let q = quiz();
    let mut e = started(&q.project);
    e.next_slide();
    e.click(&q.mr[0]);
    e.click(&q.mr[2]);
    e.click(&q.mr[2]); // toggles back off
    e.click(&q.mr[1]);
    assert_eq!(state_of(&e, &q.mr[2]), state_names::NORMAL);
    e.click(&q.submit[0]);
    assert_eq!(e.answer(&current(&e)).unwrap().result, Some(true));

    // A superset is wrong.
    let mut e = started(&q.project);
    e.next_slide();
    for id in &q.mr {
        e.click(id);
    }
    e.click(&q.submit[0]);
    assert_eq!(e.answer(&current(&e)).unwrap().result, Some(false));
}

#[test]
fn fill_in_ignores_case_and_spaces_unless_told_otherwise() {
    let q = quiz();
    let mut e = started(&q.project);
    e.go_to_slide(&q.project.scenes[0].slides[2].id);
    e.set_text_answer("  PARIS ");
    e.click(&q.submit[1]);
    assert_eq!(e.answer(&current(&e)).unwrap().result, Some(true));

    let mut p = q.project.clone();
    if let Some(Question { kind: QuestionKind::FillIn { case_sensitive, .. }, .. }) =
        &mut p.scenes[0].slides[2].question
    {
        *case_sensitive = true;
    }
    let mut e = started(&p);
    e.go_to_slide(&p.scenes[0].slides[2].id);
    e.set_text_answer("PARIS");
    e.click(&q.submit[1]);
    assert_eq!(e.answer(&current(&e)).unwrap().result, Some(false));
}

#[test]
fn matching_needs_every_pair() {
    let q = quiz();
    let mut e = started(&q.project);
    e.go_to_slide(&q.project.scenes[0].slides[3].id);
    e.set_match(0, Some(0));
    e.set_match(1, Some(2));
    e.set_match(2, Some(1));
    e.set_match(9, Some(0)); // out of range: ignored
    e.click(&q.submit[2]);
    assert_eq!(e.answer(&current(&e)).unwrap().result, Some(false));

    let mut e = started(&q.project);
    e.go_to_slide(&q.project.scenes[0].slides[3].id);
    for i in 0..3 {
        e.set_match(i, Some(i));
    }
    e.click(&q.submit[2]);
    assert_eq!(e.answer(&current(&e)).unwrap().result, Some(true));
}

/// Answer the first `correct` questions right and the rest wrong, then go to
/// the results slide.
fn take_quiz(q: &Quiz, correct: usize) -> Engine {
    let mut e = started(&q.project);
    let slides: Vec<Id> = q.project.scenes[0].slides.iter().map(|s| s.id.clone()).collect();
    // MC (wrong answers twice to use both attempts).
    if correct > 0 {
        e.click(&q.mc[0]);
        e.click(&q.mc_submit);
    } else {
        for _ in 0..2 {
            e.click(&q.mc[1]);
            e.click(&q.mc_submit);
        }
    }
    e.go_to_slide(&slides[1]);
    e.click(&q.mr[0]);
    if correct > 1 {
        e.click(&q.mr[1]);
    }
    e.click(&q.submit[0]);
    e.go_to_slide(&slides[2]);
    e.set_text_answer(if correct > 2 { "Paris" } else { "Lyon" });
    e.click(&q.submit[1]);
    e.go_to_slide(&slides[3]);
    for i in 0..3 {
        e.set_match(i, Some(if correct > 3 { i } else { 2 - i }));
    }
    e.click(&q.submit[2]);
    e.take_effects();
    e.go_to_slide(&slides[4]);
    e
}

#[test]
fn results_pass_and_report() {
    let q = quiz();
    let mut e = take_quiz(&q, 4);
    let r = e.quiz_results();
    assert_eq!((r.points, r.max_points, r.percent, r.passed, r.answered), (40.0, 40.0, 100, true, 4));
    let effects = e.take_effects();
    assert!(effects.iter().any(|f| matches!(f, Effect::QuizSubmitted(r) if r.percent == 100 && r.passed)));
    assert!(effects.contains(&Effect::Completed), "completion rule: quiz submitted");
    assert_eq!(e.variable(&q.passed_flag), Some(&Value::Bool(true)), "quiz passed condition");
    let text = e.frame().objects.iter().find(|o| o.id == q.results_text).unwrap().text.clone().unwrap().text;
    assert_eq!(text, "You scored 100% (40/40)");
}

#[test]
fn results_fail_below_passing_percent() {
    let q = quiz();
    let mut e = take_quiz(&q, 3);
    let r = e.quiz_results();
    assert_eq!((r.percent, r.passed), (75, false));
    assert!(e.take_effects().iter().any(|f| matches!(f, Effect::QuizSubmitted(r) if !r.passed)));
    assert_eq!(e.variable(&q.passed_flag), Some(&Value::Bool(false)));
}

#[test]
fn reset_results_allows_a_retake() {
    let mut q = quiz();
    q.project.scenes[0].slides[4]
        .base
        .triggers
        .push(Trigger::new(Action::ResetResults, Event::KeyPressed { key: "r".into() }));
    let mut e = take_quiz(&q, 4);
    e.key_pressed("r");
    assert_eq!(e.quiz_results().answered, 0);
    assert_eq!(state_of(&e, &q.mc[0]), state_names::NORMAL);
    e.go_to_slide(&q.project.scenes[0].slides[0].id);
    e.click(&q.mc[0]);
    assert_eq!(state_of(&e, &q.mc[0]), state_names::SELECTED, "questions are open again");
}

#[test]
fn answers_survive_resume() {
    let q = quiz();
    let e = take_quiz(&q, 2);
    let saved: Snapshot = serde_json::from_str(&serde_json::to_string(&e.snapshot()).unwrap()).unwrap();
    let mut resumed = Engine::new(q.project.clone()).unwrap();
    resumed.restore(&saved);
    resumed.start();
    assert_eq!(resumed.quiz_results(), e.quiz_results());
}

#[test]
fn invalid_questions_are_reported() {
    let mut q = quiz();
    if let Some(question) = &mut q.project.scenes[0].slides[0].question {
        if let QuestionKind::MultipleChoice { choices } = &mut question.kind {
            choices[1].correct = true; // two correct answers
            choices[2].object = Id::from("missing");
        }
        question.feedback.correct = Some(Id::from("no-layer"));
    }
    let problems = q.project.validate();
    assert_eq!(problems.len(), 3, "{problems:?}");
}
