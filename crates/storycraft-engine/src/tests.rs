use storycraft_model::sample::sample_project;
use storycraft_model::*;

use super::*;

fn object_id(p: &Project, name: &str) -> Id {
    p.slides()
        .flat_map(|(_, s)| s.layers().flat_map(|l| l.objects.iter()).collect::<Vec<_>>())
        .find(|o| o.name == name)
        .unwrap_or_else(|| panic!("no object {name}"))
        .id
        .clone()
}

fn started(p: Project) -> Engine {
    let mut e = Engine::new(p).unwrap();
    e.start();
    e
}

fn slide_name(e: &Engine) -> &str {
    &e.current_slide().unwrap().name
}

fn visible(e: &Engine, id: &Id) -> bool {
    e.frame().objects.iter().any(|o| &o.id == id)
}

fn text_of(e: &Engine, id: &Id) -> String {
    e.frame().objects.iter().find(|o| &o.id == id).unwrap().text.as_ref().unwrap().text.clone()
}

#[test]
fn starts_on_first_slide_and_jumps() {
    let p = sample_project();
    let start = object_id(&p, "Start Button");
    let mut e = started(p);
    assert_eq!(slide_name(&e), "Welcome");
    assert!(!e.can_go_back());

    e.click(&start);
    assert_eq!(slide_name(&e), "Counter");
    assert!(e.can_go_back());

    e.previous_slide();
    assert_eq!(slide_name(&e), "Welcome");
    assert!(!e.can_go_back(), "going back pops history");
}

#[test]
fn objects_appear_on_their_timeline() {
    let p = sample_project();
    let subtitle = object_id(&p, "Subtitle");
    let mut e = started(p);
    assert!(!visible(&e, &subtitle));
    e.tick(499.0);
    assert!(!visible(&e, &subtitle));
    e.tick(1.0);
    assert!(visible(&e, &subtitle));
}

#[test]
fn layers_show_and_hide() {
    let p = sample_project();
    let (more, close, panel) = (object_id(&p, "More Button"), object_id(&p, "Close Button"), object_id(&p, "Panel"));
    let mut e = started(p);
    assert!(!visible(&e, &panel));
    e.click(&more);
    assert!(visible(&e, &panel));
    assert_eq!(e.layer_runs().len(), 2);
    e.click(&close);
    assert!(!visible(&e, &panel));
    assert_eq!(e.layer_runs().len(), 1);
}

#[test]
fn variables_drive_text_and_conditions() {
    let p = sample_project();
    let (start, plus, star, readout) =
        (object_id(&p, "Start Button"), object_id(&p, "Plus Button"), object_id(&p, "Star"), object_id(&p, "Readout"));
    let mut e = started(p);
    e.click(&start);
    assert_eq!(text_of(&e, &readout), "Clicks: 0");
    e.click(&plus);
    e.click(&plus);
    assert_eq!(text_of(&e, &readout), "Clicks: 2");
    assert!(!visible(&e, &star), "star stays hidden until Count >= 3");
    e.click(&plus);
    assert_eq!(text_of(&e, &readout), "Clicks: 3");
    assert!(visible(&e, &star));
}

#[test]
fn completion_when_all_slides_viewed() {
    let mut e = started(sample_project());
    assert!(!e.is_completed());
    e.next_slide();
    e.next_slide();
    assert!(e.is_completed());
    assert!(e.take_effects().contains(&Effect::Completed));
    // Never reported twice.
    e.previous_slide();
    e.next_slide();
    assert!(!e.take_effects().contains(&Effect::Completed));
}

#[test]
fn completion_on_trigger_at_timeline_end() {
    let mut p = sample_project();
    p.lms.completion = Completion::OnTrigger;
    let mut e = started(p);
    e.next_slide();
    e.next_slide();
    assert_eq!(slide_name(&e), "Done");
    assert!(!e.is_completed());
    e.tick(2999.0);
    assert!(!e.is_completed());
    e.tick(5.0);
    assert!(e.is_completed());
}

#[test]
fn automatic_advance() {
    let mut p = sample_project();
    p.scenes[0].slides[0].advance = Advance::Automatically;
    p.scenes[0].slides[0].base.duration_ms = 1000;
    let mut e = started(p);
    e.tick(999.0);
    assert_eq!(slide_name(&e), "Welcome");
    e.tick(1.0);
    assert_eq!(slide_name(&e), "Counter");
}

#[test]
fn layer_can_pause_base_timeline() {
    let mut p = sample_project();
    p.scenes[0].slides[0].layers[0].pause_base_timeline = true;
    let more = object_id(&p, "More Button");
    let close = object_id(&p, "Close Button");
    let mut e = started(p);
    e.tick(100.0);
    e.click(&more);
    e.tick(1000.0);
    assert_eq!(e.layer_runs()[0].time_ms, 100.0);
    e.click(&close);
    e.tick(50.0);
    assert_eq!(e.layer_runs()[0].time_ms, 150.0);
}

#[test]
fn hover_and_down_states_via_pointer() {
    let p = sample_project();
    let start = object_id(&p, "Start Button");
    let mut e = started(p);
    let state = |e: &Engine| e.frame().objects.iter().find(|o| o.id == start).unwrap().state.clone();
    let inside = Point { x: 640.0, y: 450.0 };

    e.pointer_move(Some(inside));
    assert_eq!(state(&e), state_names::HOVER);
    e.pointer_down(inside);
    assert_eq!(state(&e), state_names::DOWN);
    e.pointer_up(inside);
    assert_eq!(slide_name(&e), "Counter", "press + release over the button clicks it");
}

#[test]
fn release_elsewhere_does_not_click() {
    let mut e = started(sample_project());
    e.pointer_down(Point { x: 640.0, y: 450.0 });
    e.pointer_up(Point { x: 10.0, y: 10.0 });
    assert_eq!(slide_name(&e), "Welcome");
}

#[test]
fn non_interactive_objects_do_not_block_clicks() {
    let mut p = sample_project();
    // Put a plain text box on top of the Start button.
    let mut label = Object::new("Label", ObjectKind::Text, Rect::new(500.0, 400.0, 300.0, 100.0));
    label.text = Some(TextContent::new("over the button"));
    p.scenes[0].slides[0].base.objects.push(label);
    let mut e = started(p);
    let inside = Point { x: 640.0, y: 450.0 };
    e.pointer_down(inside);
    e.pointer_up(inside);
    assert_eq!(slide_name(&e), "Counter");
}

#[test]
fn hidden_and_disabled_objects_ignore_clicks() {
    let p = sample_project();
    let start = object_id(&p, "Start Button");
    let mut e = started(p.clone());
    e.states.insert(start.clone(), state_names::DISABLED.into());
    e.click(&start);
    assert_eq!(slide_name(&e), "Welcome");
    e.states.insert(start.clone(), state_names::HIDDEN.into());
    e.click(&start);
    assert_eq!(slide_name(&e), "Welcome");
}

fn var_project(clauses: Vec<ConditionClause>, vars: &[(&str, Value)]) -> (Project, Id, Id) {
    let mut p = Project::new("t");
    let ids: Vec<_> = vars
        .iter()
        .map(|(n, v)| {
            let var = Variable::new(*n, v.clone());
            let id = var.id.clone();
            p.variables.push(var);
            id
        })
        .collect();
    let result = Variable::new("Result", Value::Bool(false));
    let result_id = result.id.clone();
    p.variables.push(result);
    let mut t = Trigger::new(
        Action::SetVariable {
            variable: result_id.clone(),
            op: VarOp::Assign,
            value: Operand::Literal(Value::Bool(true)),
        },
        Event::KeyPressed { key: "Enter".into() },
    );
    t.conditions = clauses;
    p.scenes[0].slides[0].base.triggers.push(t);
    (p, ids[0].clone(), result_id)
}

fn cond(var: &Id, cmp: Comparison, v: Value, join: Join) -> ConditionClause {
    ConditionClause { join, condition: Condition::Variable { variable: var.clone(), cmp, value: Operand::Literal(v) } }
}

#[test]
fn and_binds_tighter_than_or() {
    // false OR (true AND true) → true
    let (mut p, a, result) = var_project(vec![], &[("A", Value::Number(5.0))]);
    let t = &mut p.scenes[0].slides[0].base.triggers[0];
    t.conditions = vec![
        cond(&a, Comparison::Eq, Value::Number(1.0), Join::And),
        cond(&a, Comparison::Gt, Value::Number(1.0), Join::Or),
        cond(&a, Comparison::Lt, Value::Number(10.0), Join::And),
    ];
    let mut e = started(p.clone());
    e.key_pressed("Enter");
    assert_eq!(e.variable(&result), Some(&Value::Bool(true)));

    // (true AND false) OR false → false
    let t = &mut p.scenes[0].slides[0].base.triggers[0];
    t.conditions = vec![
        cond(&a, Comparison::Eq, Value::Number(5.0), Join::And),
        cond(&a, Comparison::Gt, Value::Number(10.0), Join::And),
        cond(&a, Comparison::Eq, Value::Number(0.0), Join::Or),
    ];
    let mut e = started(p);
    e.key_pressed("Enter");
    assert_eq!(e.variable(&result), Some(&Value::Bool(false)));
}

#[test]
fn comparisons() {
    let n = |x: f64| Value::Number(x);
    let t = |s: &str| Value::Text(s.into());
    assert!(compare(&n(10.0), Comparison::Gt, &n(9.0)));
    assert!(compare(&t("10"), Comparison::Gt, &n(9.0)), "numeric when one side is a number");
    assert!(compare(&t("10"), Comparison::Lt, &t("9")), "text compares as text");
    assert!(compare(&t("Hello"), Comparison::EqIgnoreCase, &t("hELLO")));
    assert!(compare(&t("Hello"), Comparison::Contains, &t("ell")));
    assert!(compare(&Value::Bool(true), Comparison::Eq, &t("true")));
    assert!(!compare(&n(1.0), Comparison::Eq, &n(f64::NAN)));
}

#[test]
fn variable_ops() {
    let n = |x: f64| Value::Number(x);
    assert_eq!(apply_op(&n(2.0), VarOp::Add, &n(3.0)), Some(n(5.0)));
    assert_eq!(apply_op(&n(2.0), VarOp::Divide, &n(0.0)), None, "division by zero is ignored");
    assert_eq!(apply_op(&Value::Text("ab".into()), VarOp::Add, &n(1.0)), Some(Value::Text("ab1".into())));
    assert_eq!(apply_op(&Value::Bool(true), VarOp::Toggle, &n(0.0)), Some(Value::Bool(false)));
    assert_eq!(apply_op(&n(1.0), VarOp::Assign, &Value::Text("7".into())), Some(n(7.0)));
}

#[test]
fn single_letter_keys_ignore_case() {
    assert!(keys_match("a", "A"));
    assert!(keys_match("Enter", "Enter"));
    assert!(!keys_match("Enter", "enter"));
}

#[test]
fn variable_change_loops_are_bounded() {
    let mut p = Project::new("loop");
    let v = Variable::new("N", Value::Number(0.0));
    let id = v.id.clone();
    p.variables.push(v);
    // When N changes, add 1 to N: an infinite cascade.
    p.scenes[0].slides[0].base.triggers.push(Trigger::new(
        Action::SetVariable { variable: id.clone(), op: VarOp::Add, value: Operand::Literal(Value::Number(1.0)) },
        Event::VariableChanges { variable: id.clone() },
    ));
    p.scenes[0].slides[0].base.triggers.push(Trigger::new(
        Action::SetVariable { variable: id.clone(), op: VarOp::Add, value: Operand::Literal(Value::Number(1.0)) },
        Event::KeyPressed { key: "x".into() },
    ));
    let mut e = started(p);
    e.key_pressed("x");
    let n = e.variable(&id).unwrap().as_number().unwrap();
    assert!(n > 1.0 && n <= MAX_CASCADE as f64 + 1.0);
}

#[test]
fn jump_stops_remaining_triggers() {
    let mut p = sample_project();
    let start = object_id(&p, "Start Button");
    let count = p.variables[0].id.clone();
    // A second trigger on the same click, after the jump, must not run.
    p.scenes[0].slides[0].base.triggers.push(Trigger::new(
        Action::SetVariable {
            variable: count.clone(),
            op: VarOp::Assign,
            value: Operand::Literal(Value::Number(99.0)),
        },
        Event::Click { object: start.clone() },
    ));
    let mut e = started(p);
    e.click(&start);
    assert_eq!(e.variable(&count), Some(&Value::Number(0.0)));
}

#[test]
fn snapshot_restores_progress() {
    let p = sample_project();
    let (start, plus, star) = (object_id(&p, "Start Button"), object_id(&p, "Plus Button"), object_id(&p, "Star"));
    let mut e = started(p.clone());
    e.click(&start);
    for _ in 0..3 {
        e.click(&plus);
    }
    let snap = e.snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let snap: Snapshot = serde_json::from_str(&json).unwrap();

    let mut resumed = Engine::new(p).unwrap();
    resumed.restore(&snap);
    resumed.start();
    assert_eq!(slide_name(&resumed), "Counter");
    assert!(visible(&resumed, &star));
    assert_eq!(resumed.snapshot().variables, e.snapshot().variables);
}

#[test]
fn selected_state_toggles_on_click() {
    let mut p = sample_project();
    let plus = object_id(&p, "Plus Button");
    p.scenes[0].slides[1].base.objects[2].states.push(ObjectState::new(state_names::SELECTED));
    let mut e = started(p);
    e.next_slide();
    e.click(&plus);
    let state = |e: &Engine| e.frame().objects.iter().find(|o| o.id == plus).unwrap().state.clone();
    assert_eq!(state(&e), state_names::SELECTED);
    e.click(&plus);
    assert_eq!(state(&e), state_names::NORMAL);
}

#[test]
fn rotated_hit_testing() {
    let mut o = Object::new("r", ObjectKind::Button, Rect::new(0.0, 0.0, 200.0, 20.0));
    // Centre (100, 10); a corner point is outside once rotated 90°.
    assert!(contains_rotated(&o, Point { x: 5.0, y: 5.0 }));
    o.rotation = 90.0;
    assert!(!contains_rotated(&o, Point { x: 5.0, y: 5.0 }));
    assert!(contains_rotated(&o, Point { x: 100.0, y: 90.0 }));
}

#[test]
fn empty_project_is_an_error() {
    let mut p = Project::new("empty");
    p.scenes.clear();
    assert_eq!(Engine::new(p).err(), Some(EngineError::NoSlides));
}
