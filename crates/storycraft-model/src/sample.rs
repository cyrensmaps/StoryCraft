//! A small demonstration course that exercises layers, states, variables and
//! timing. Used by tests, by `storycraft-cli sample`, and as the editor's
//! "New from sample".

use crate::*;

const ACCENT: Color = Color::rgb(0x2B, 0x6C, 0xB0);
const ACCENT_HOVER: Color = Color::rgb(0x3A, 0x82, 0xCF);
const ACCENT_DOWN: Color = Color::rgb(0x1E, 0x4E, 0x80);

fn text(name: &str, body: &str, size: f32, frame: Rect) -> Object {
    let mut o = Object::new(name, ObjectKind::Text, frame);
    let mut t = TextContent::new(body);
    t.font_size = size;
    o.text = Some(t);
    o
}

fn button(name: &str, label: &str, frame: Rect) -> Object {
    let mut o = Object::new(name, ObjectKind::Button, frame);
    o.style.fill = Some(ACCENT);
    o.style.corner_radius = 8.0;
    let mut t = TextContent::new(label);
    t.color = Color::WHITE;
    t.bold = true;
    o.text = Some(t);
    o.alt_text = label.to_owned();
    o.states = vec![
        ObjectState {
            name: state_names::HOVER.into(),
            overrides: StateOverrides { fill: Some(ACCENT_HOVER), ..Default::default() },
        },
        ObjectState {
            name: state_names::DOWN.into(),
            overrides: StateOverrides { fill: Some(ACCENT_DOWN), ..Default::default() },
        },
    ];
    o
}

fn on_click(object: &Object, action: Action) -> Trigger {
    Trigger::new(action, Event::Click { object: object.id.clone() })
}

pub fn sample_project() -> Project {
    let mut p = Project::new("StoryCraft Sample Course");
    p.author = "StoryCraft".into();
    let count = Variable::new("Count", Value::Number(0.0));
    let count_id = count.id.clone();
    p.variables.push(count);

    // --- Slide 1: welcome, with a pop-up layer --------------------------------
    let mut welcome = Slide::new("Welcome");
    welcome.background = Color::rgb(0xF4, 0xF7, 0xFB);
    let title = text("Title", "Welcome to StoryCraft", 54.0, Rect::new(140.0, 160.0, 1000.0, 100.0));
    let mut subtitle =
        text("Subtitle", "An open-source e-learning authoring tool", 28.0, Rect::new(140.0, 260.0, 1000.0, 60.0));
    subtitle.timing.start_ms = 500;
    let start = button("Start Button", "Start", Rect::new(540.0, 420.0, 200.0, 64.0));
    let more = button("More Button", "Learn more", Rect::new(540.0, 500.0, 200.0, 64.0));

    let mut info = Layer::new("Info");
    let mut panel =
        Object::new("Panel", ObjectKind::Shape { shape: ShapeKind::Rectangle }, Rect::new(290.0, 140.0, 700.0, 440.0));
    panel.style.fill = Some(Color::WHITE);
    panel.style.stroke = Some(Stroke { color: ACCENT, width: 3.0 });
    panel.style.corner_radius = 16.0;
    let mut panel_text = text(
        "Panel Text",
        "Slides hold layers. This pop-up is a layer that a trigger showed.",
        26.0,
        Rect::new(330.0, 190.0, 620.0, 240.0),
    );
    panel_text.text.as_mut().unwrap().align = HAlign::Left;
    let close = button("Close Button", "Close", Rect::new(540.0, 480.0, 200.0, 64.0));
    info.triggers.push(on_click(&close, Action::HideLayer { layer: LayerTarget::This }));
    info.objects = vec![panel, panel_text, close];

    welcome.base.triggers.push(on_click(&start, Action::JumpToSlide { target: SlideTarget::Next }));
    welcome.base.triggers.push(on_click(&more, Action::ShowLayer { layer: info.id.clone() }));
    welcome.base.objects = vec![title, subtitle, start, more];
    welcome.layers.push(info);

    // --- Slide 2: a variable-driven counter ----------------------------------
    let mut counter = Slide::new("Counter");
    let heading = text("Heading", "Click the button three times", 40.0, Rect::new(140.0, 80.0, 1000.0, 80.0));
    let readout = text("Readout", "Clicks: %Count%", 32.0, Rect::new(140.0, 180.0, 1000.0, 60.0));
    let plus = button("Plus Button", "+1", Rect::new(540.0, 280.0, 200.0, 64.0));
    let mut star =
        Object::new("Star", ObjectKind::Shape { shape: ShapeKind::Triangle }, Rect::new(580.0, 390.0, 120.0, 110.0));
    star.style.fill = Some(Color::rgb(0xF2, 0xB7, 0x05));
    star.initially_hidden = true;
    let next = button("Next Button", "Next", Rect::new(1040.0, 620.0, 200.0, 64.0));

    counter.base.triggers.push(on_click(
        &plus,
        Action::SetVariable { variable: count_id.clone(), op: VarOp::Add, value: Operand::Literal(Value::Number(1.0)) },
    ));
    let mut reveal = Trigger::new(
        Action::ChangeState { object: star.id.clone(), state: state_names::NORMAL.into() },
        Event::VariableChanges { variable: count_id.clone() },
    );
    reveal.conditions.push(ConditionClause {
        join: Join::And,
        condition: Condition::Variable {
            variable: count_id,
            cmp: Comparison::Ge,
            value: Operand::Literal(Value::Number(3.0)),
        },
    });
    counter.base.triggers.push(reveal);
    counter.base.triggers.push(on_click(&next, Action::JumpToSlide { target: SlideTarget::Next }));
    counter.base.objects = vec![heading, readout, plus, star, next];

    // --- Slide 3: timed reveal and completion --------------------------------
    let mut done = Slide::new("Done");
    done.background = Color::rgb(0xE8, 0xF5, 0xE9);
    let mut well = text("Well Done", "Well done!", 64.0, Rect::new(140.0, 220.0, 1000.0, 120.0));
    well.timing.start_ms = 1000;
    let mut summary = text("Summary", "You clicked %Count% times.", 28.0, Rect::new(140.0, 340.0, 1000.0, 60.0));
    summary.timing.start_ms = 2000;
    done.base.duration_ms = 3000;
    done.base.triggers.push(Trigger::new(Action::CompleteCourse, Event::TimelineEnds));
    done.base.objects = vec![well, summary];

    p.scenes[0].name = "Introduction".into();
    p.scenes[0].slides = vec![welcome, counter, done];
    p
}

#[cfg(test)]
mod tests {
    #[test]
    fn sample_is_consistent() {
        let p = super::sample_project();
        assert_eq!(p.validate(), Vec::<String>::new());
        let json = serde_json::to_string(&p).unwrap();
        let back: crate::Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }
}
