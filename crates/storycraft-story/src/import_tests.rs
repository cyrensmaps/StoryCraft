//! Importer tests against small synthetic packages. They are written by hand
//! from the documented structure; real `.story` files are never committed.

use std::io::{Cursor, Write};

use storycraft_model::*;

use crate::import::import;

const RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

struct Pkg(zip::ZipWriter<Cursor<Vec<u8>>>);

impl Pkg {
    fn new() -> Self {
        Pkg(zip::ZipWriter::new(Cursor::new(Vec::new())))
    }

    fn file(mut self, path: &str, body: &[u8]) -> Self {
        self.0.start_file(path, zip::write::SimpleFileOptions::default()).unwrap();
        self.0.write_all(body).unwrap();
        self
    }

    fn import(self) -> crate::Imported {
        let bytes = self.0.finish().unwrap().into_inner();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        import(&mut zip, "Test").unwrap()
    }
}

fn rels(entries: &[(&str, &str, &str)]) -> Vec<u8> {
    let body: String = entries
        .iter()
        .map(|(id, ty, target)| format!(r#"<Relationship Type="{ty}" Target="{target}" Id="{id}"/>"#))
        .collect();
    format!(r#"<?xml version="1.0"?><Relationships xmlns="{RELS_NS}">{body}</Relationships>"#).into_bytes()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn text_doc(text: &str) -> String {
    escape(&format!(r#"<Document><Content><Block><Span Text="{}"/></Block></Content></Document>"#, escape(text)))
}

const SCORE: &str = "11111111-0000-0000-0000-000000000001";
const DONE: &str = "11111111-0000-0000-0000-000000000002";
const BUTTON: &str = "22222222-0000-0000-0000-000000000001";
const BOX: &str = "22222222-0000-0000-0000-000000000002";
const LAYER: &str = "33333333-0000-0000-0000-000000000001";
const LAYOUT: &str = "44444444-0000-0000-0000-000000000001";

fn story_xml(scenes: &str) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0"?><story>
        <varLst>
          <var g="{SCORE}" name="Score" dataType="num" val="5" type="user"/>
          <var g="{DONE}" name="Done" dataType="bool" val="False" type="user"/>
          <var g="99999999-0000-0000-0000-000000000000" name="Project.SlideNumber" dataType="num" val="0" type="builtIn"/>
        </varLst>
        <sceneLst>{scenes}</sceneLst>
        <propLst><prop id="15"><sz w="720" h="540"/></prop><prop id="18">Author Name</prop></propLst>
        </story>"#
    )
    .into_bytes()
}

fn base_package(slide: &str) -> Pkg {
    package(
        &story_xml(r#"<scene g="s1" name="Intro" sceneType="scene"><sldIdLst><sldId>Rs1</sldId></sldIdLst></scene>"#),
        slide,
    )
}

fn package(story: &[u8], slide: &str) -> Pkg {
    Pkg::new()
        .file("_rels/.rels", &rels(&[("R1", "main", "/story/story.xml")]))
        .file("story/story.xml", story)
        .file("story/_rels/story.xml.rels", &rels(&[("Rs1", "slide", "/story/slides/slide.xml")]))
        .file(
            "story/slideMasters/slideMaster.xml",
            br#"<master g="m1"><clrLst><dk1><srgbClr val="000000"/></dk1><lt1><srgbClr val="FFFFFF"/></lt1><accent1><srgbClr val="4F81BD"/></accent1><custom1><srgbClr val="1888E7"/></custom1></clrLst></master>"#,
        )
        .file(
            "story/slideMasters/_rels/slideMaster.xml.rels",
            &rels(&[("Rl1", "slideLayout", "/story/slideLayouts/slideLayout.xml")]),
        )
        .file("story/slideLayouts/slideLayout.xml", format!(r#"<layout g="{LAYOUT}"/>"#).as_bytes())
        .file("story/slides/slide.xml", slide.as_bytes())
}

fn full_slide() -> String {
    let label = text_doc("Count: %Score%");
    let and = text_doc("AND");
    format!(
        r#"<sld g="slide-1" name="First" advMode="user" layoutG="{LAYOUT}">
        <tmProps min="4000"/>
        <shapeLst>
          <textBox g="text-1" typeName="" name="Text Box" initState="">
            <tmCtxLst><txtTmCtx start="1000" dur="2000" untilEnd="false"/></tmCtxLst>
            <loc l="10" t="20" r="210" b="70"/>
            <text>{label}</text><plain>Count: %Score%</plain><resize fontNm="+minor" fontSz="12"/>
          </textBox>
          <rect g="{BOX}" typeName="Box" name="Rectangle" initState="Hidden">
            <tmCtxLst><txtTmCtx start="0" dur="5000" untilEnd="true"/></tmCtxLst>
            <bG><solidFill><clr><schemeClr val="accent1"/></clr></solidFill><noLine/></bG>
            <loc l="100" t="100" r="200" b="150"/>
          </rect>
          <btn g="{BUTTON}" typeName="AND" name="Button" initState="Normal">
            <tmCtxLst><txtTmCtx start="0" dur="5000" untilEnd="true"/></tmCtxLst>
            <trigLst>
              <trig g="trig-1">
                <data enabled="true" event="OnClick" action="jumpToSlide" actSubType="next" NavigationIntent="Unspecified"/>
                <condLst>
                  <trigCond op="gt" floatVal1="3" boolVal="" strVal1="" varG="{SCORE}" varG2="00000000-0000-0000-0000-000000000000" dataType="value" andOr="and"/>
                  <trigCond op="eq" floatVal1="0" boolVal="True" strVal1="" varG="{DONE}" varG2="00000000-0000-0000-0000-000000000000" dataType="value" andOr="or"/>
                  <trigCond op="eq" dataType="state" andOr="and"><shapeState shapeG="{BOX}" op="noteq" name="Normal"/></trigCond>
                </condLst>
              </trig>
              <trig g="trig-2">
                <data enabled="true" event="OnClick" action="adjustVar" actSubType="next" NavigationIntent="Unspecified">
                  <other varG="{SCORE}" op="add" dblVal="2" boolVal="True" varVal1="" useVar="false"/>
                </data>
              </trig>
              <trig g="trig-3">
                <data enabled="true" event="OnClick" action="changeShapeState" NavigationIntent="Unspecified">
                  <shape setStateG="{BOX}" setStateName="Normal"/>
                </data>
              </trig>
            </trigLst>
            <stateLst>
              <state name="Normal"><shapeLst><btn g="n">
                <bG><solidFill><clr><schemeClr val="custom1"/></clr></solidFill><foreClr><schemeClr val="lt1"/></foreClr></bG>
                <loc l="0" t="0" r="100" b="30"/><text>{and}</text>
              </btn></shapeLst></state>
              <state name="Hover"><shapeLst><btn g="h">
                <bG><solidFill><clr><tint val="50000"/></clr></solidFill></bG>
                <loc l="0" t="0" r="100" b="30"/><text>{and}</text>
              </btn></shapeLst></state>
            </stateLst>
            <loc l="300" t="100" r="400" b="130"/>
          </btn>
        </shapeLst>
        <trigLst>
          <trig g="nav"><data enabled="true" event="OnNextButtonClick" action="jumpToSlide" actSubType="next" NavigationIntent="Next"/></trig>
          <trig g="start"><data enabled="true" event="OnStart" action="adjustVar" NavigationIntent="Unspecified"><other varG="{SCORE}" op="ass" dblVal="0"/></data></trig>
        </trigLst>
        <sldLayerLst>
          <sldLayer g="{LAYER}" name="Popup" pause="true" stop="false">
            <tmProps min="3000"/>
            <shapeLst>
              <oval g="close-1" typeName="" name="Oval">
                <trigLst><trig g="trig-4"><data enabled="true" event="OnClick" action="hideSubSlide" actSubType="me" NavigationIntent="Unspecified"><sldLayer hideG="{LAYER}"/></data></trig></trigLst>
                <loc l="0" t="0" r="50" b="50"/>
              </oval>
            </shapeLst>
          </sldLayer>
        </sldLayerLst>
        </sld>"#
    )
}

#[test]
fn imports_project_structure() {
    let imported = base_package(&full_slide()).import();
    let p = &imported.document.project;
    assert_eq!(p.slide_size, Size { width: 720.0, height: 540.0 });
    assert_eq!(p.author, "Author Name");
    assert_eq!(p.scenes.len(), 1);
    assert_eq!(p.scenes[0].name, "Intro");
    let names: Vec<_> = p.variables.iter().map(|v| (v.name.as_str(), v.value.clone())).collect();
    assert_eq!(
        names,
        vec![("Score", Value::Number(5.0)), ("Done", Value::Bool(false))],
        "built-in variables are skipped"
    );

    let slide = &p.scenes[0].slides[0];
    assert_eq!(slide.name, "First");
    assert_eq!(slide.advance, Advance::ByUser);
    assert_eq!(slide.base.duration_ms, 4000);
    assert_eq!(slide.layers.len(), 1);
    assert_eq!(slide.layers[0].name, "Popup");
    assert!(slide.layers[0].pause_base_timeline);
    assert!(!slide.layers[0].hide_other_layers, "stop=false turns off hiding other layers");
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
}

#[test]
fn imports_objects() {
    let p = base_package(&full_slide()).import().document.project;
    let objects = &p.scenes[0].slides[0].base.objects;

    let text = &objects[0];
    assert_eq!(text.kind, ObjectKind::Text);
    assert_eq!(text.frame, Rect::new(10.0, 20.0, 200.0, 50.0));
    assert_eq!(text.text.as_ref().unwrap().text, "Count: %Score%");
    assert_eq!(text.timing, Timing { start_ms: 1000, duration_ms: Some(2000) });
    assert_eq!(text.text.as_ref().unwrap().font_size, 12.0, "unstyled text uses resize@fontSz");
    assert_eq!(text.text.as_ref().unwrap().font_family, "sans-serif", "theme font names keep the default");

    let rect = &objects[1];
    assert_eq!(rect.name, "Box", "the author's name wins over the default");
    assert_eq!(rect.style.fill, Some(Color::rgb(0x4F, 0x81, 0xBD)), "scheme colours resolve via the layout's master");
    assert!(rect.initially_hidden);
    assert_eq!(rect.timing.duration_ms, None, "untilEnd means open-ended");

    let button = &objects[2];
    assert_eq!(button.kind, ObjectKind::Button);
    assert_eq!(button.style.fill, Some(Color::rgb(0x18, 0x88, 0xE7)), "look comes from the Normal state");
    assert_eq!(button.text.as_ref().unwrap().text, "AND");
    assert_eq!(button.text.as_ref().unwrap().color, Color::WHITE);
    let hover = button.state(state_names::HOVER).unwrap();
    let expected = Color::rgb(0x8C, 0xC4, 0xF3); // 50% tint of #1888E7
    assert_eq!(hover.overrides.fill, Some(expected), "relative state colours apply to the Normal fill");
}

#[test]
fn imports_triggers_and_conditions() {
    let p = base_package(&full_slide()).import().document.project;
    let slide = &p.scenes[0].slides[0];
    let base = &slide.base.triggers;
    // The player's built-in Next trigger is not imported; OnStart is.
    assert!(base.iter().any(|t| t.event == Event::TimelineStarts));
    assert!(!base.iter().any(|t| t.id.as_str() == "nav"));

    let jump = base.iter().find(|t| t.id.as_str() == "trig-1").unwrap();
    assert_eq!(jump.event, Event::Click { object: Id::from(BUTTON) });
    assert_eq!(jump.action, Action::JumpToSlide { target: SlideTarget::Next });
    assert_eq!(jump.conditions.len(), 3);
    assert!(matches!(
        &jump.conditions[0].condition,
        Condition::Variable { cmp: Comparison::Gt, value: Operand::Literal(Value::Number(n)), .. } if *n == 3.0
    ));
    assert_eq!(jump.conditions[1].join, Join::Or);
    assert!(matches!(
        &jump.conditions[2].condition,
        Condition::ObjectState { negate: true, state, .. } if state == "Normal"
    ));

    let add = base.iter().find(|t| t.id.as_str() == "trig-2").unwrap();
    assert_eq!(
        add.action,
        Action::SetVariable { variable: Id::from(SCORE), op: VarOp::Add, value: Operand::Literal(Value::Number(2.0)) }
    );
    let show = base.iter().find(|t| t.id.as_str() == "trig-3").unwrap();
    assert_eq!(show.action, Action::ChangeState { object: Id::from(BOX), state: "Normal".into() });

    let hide = &slide.layers[0].triggers[0];
    assert_eq!(hide.action, Action::HideLayer { layer: LayerTarget::This });
}

#[test]
fn imported_course_runs() {
    let p = base_package(&full_slide()).import().document.project;
    let mut engine = storycraft_engine::Engine::new(p).unwrap();
    engine.start();
    let score = Id::from(SCORE);
    assert_eq!(engine.variable(&score), Some(&Value::Number(0.0)), "the OnStart trigger reset Score");
    engine.click(&Id::from(BUTTON));
    assert_eq!(engine.variable(&score), Some(&Value::Number(2.0)));
}

#[test]
fn media_is_matched_by_checksum() {
    let png = b"\x89PNG\r\n\x1a\nfake image";
    let hash: String = <md5::Md5 as md5::Digest>::digest(png).iter().map(|b| format!("{b:02x}")).collect();
    let story = String::from_utf8(story_xml(
        r#"<scene g="s1" name="" sceneType="scene"><sldIdLst><sldId>Rs1</sldId></sldIdLst></scene>"#,
    ))
    .unwrap()
    .replace(
        "<sceneLst>",
        &format!(
            r#"<mediaLst><mediaLst><media g="media-1" displayName="photo.png"><md5Checksum><stream>{hash}</stream></md5Checksum></media></mediaLst></mediaLst><sceneLst>"#
        ),
    );
    let slide = r#"<sld g="s" name="" layoutG="x"><tmProps min="5000"/><shapeLst>
        <pic g="pic-1" typeName="" name="Picture" assetG="media-1"><loc l="0" t="0" r="10" b="10"/></pic>
        </shapeLst></sld>"#;
    let imported = package(story.as_bytes(), slide).file("story/media/R6abc.png", png).import();
    let doc = &imported.document;
    assert_eq!(doc.project.assets.len(), 1);
    assert_eq!(doc.project.assets[0].name, "photo.png");
    assert_eq!(doc.media.values().next().unwrap().as_slice(), png);
    let pic = &doc.project.scenes[0].slides[0].base.objects[0];
    assert_eq!(pic.kind, ObjectKind::Image { asset: doc.project.assets[0].id.clone() });
    assert_eq!(doc.project.scenes[0].name, "Scene 1", "unnamed scenes get a default name");
}

#[test]
fn unknown_things_become_warnings() {
    let slide = r#"<sld g="s" name="" layoutG="x"><shapeLst>
        <star5 g="star" typeName="" name="Star"><loc l="0" t="0" r="10" b="10"/></star5>
        </shapeLst><trigLst>
        <trig g="t"><data enabled="true" event="OnTimerEnd" action="jumpToSlide" actSubType="next" NavigationIntent="Unspecified"/></trig>
        </trigLst></sld>"#;
    let imported = base_package(slide).import();
    assert_eq!(imported.document.project.scenes[0].slides[0].base.objects.len(), 1);
    assert!(imported.warnings.iter().any(|w| w.contains("<star5>")));
    assert!(imported.warnings.iter().any(|w| w.contains("OnTimerEnd")));
}

#[test]
fn imports_formatting_outlines_and_more_events() {
    let styled = escape(
        r##"<Document><Content><Block><Style Justification="Center"/><Span Text="Red"><Style FontFamily="Arial" FontSize="36" FontIsBold="True" ForegroundColor="#FF0000"/></Span></Block></Content></Document>"##,
    );
    let slide = format!(
        r#"<sld g="slide-a" name="A" layoutG="{LAYOUT}"><tmProps min="5000"/>
        <shapeLst>
          <textBox g="t1" typeName="" name="Text Box"><loc l="0" t="0" r="100" b="40"/><text>{styled}</text></textBox>
          <rect g="r1" typeName="" name="Rectangle">
            <bG><solidFill><clr><schemeClr val="accent1"/></clr></solidFill><solidLine><clr><srgbClr val="8F0000"/></clr></solidLine><lineStyle w="7"/></bG>
            <loc l="0" t="50" r="100" b="100"/>
          </rect>
        </shapeLst>
        <trigLst>
          <trig g="e1"><data enabled="true" event="OnEnd" action="adjustVar" NavigationIntent="Unspecified"><other varG="{SCORE}" op="ass" dblVal="1"/></data></trig>
          <trig g="e2"><data enabled="true" event="OnKeyPress" action="jumpToSlide" actSubType="spec" NavigationIntent="Unspecified"><keyPress keys="Enter"/><slide jumpG="slide-b"/></data></trig>
          <trig g="e3"><data enabled="true" event="OnVariableValueChange" action="jumpToSlide" actSubType="spec" NavigationIntent="Unspecified"><slide jumpG="slide-b"/><other varChangeG="{SCORE}"/></data></trig>
        </trigLst></sld>"#
    );
    let p = base_package(&slide).import().document.project;
    let base = &p.scenes[0].slides[0].base;

    let text = base.objects[0].text.as_ref().unwrap();
    assert_eq!((text.align, text.bold, text.color), (HAlign::Center, true, Color::rgb(255, 0, 0)));
    assert_eq!(text.font_size, 36.0);
    assert_eq!(text.font_family, "Arial");
    assert_eq!(base.objects[1].style.stroke, Some(Stroke { color: Color::rgb(0x8F, 0, 0), width: 7.0 }));

    let event = |id: &str| base.triggers.iter().find(|t| t.id.as_str() == id).unwrap();
    assert_eq!(event("e1").event, Event::TimelineEnds);
    assert_eq!(event("e2").event, Event::KeyPressed { key: "Enter".into() });
    assert_eq!(event("e2").action, Action::JumpToSlide { target: SlideTarget::Slide(Id::from("slide-b")) });
    assert_eq!(event("e3").event, Event::VariableChanges { variable: Id::from(SCORE) });
}
