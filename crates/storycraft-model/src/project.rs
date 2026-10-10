use serde::{Deserialize, Serialize};

use crate::{Color, Id, Object, Size, Trigger, Variable};

/// Bumped whenever the serialised layout changes incompatibly.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub id: Id,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    pub slide_size: Size,
    #[serde(default)]
    pub variables: Vec<Variable>,
    pub scenes: Vec<Scene>,
    /// Media referenced by objects. The bytes live alongside the project in
    /// its archive; see `archive::Document`.
    #[serde(default)]
    pub assets: Vec<Asset>,
    /// First slide shown. Defaults to the first slide of the first scene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_slide: Option<Id>,
    #[serde(default)]
    pub lms: LmsSettings,
    #[serde(default)]
    pub player: PlayerSettings,
}

impl Project {
    pub fn new(title: impl Into<String>) -> Self {
        Project {
            format_version: FORMAT_VERSION,
            id: Id::new(),
            title: title.into(),
            author: String::new(),
            slide_size: Size::default(),
            variables: Vec::new(),
            scenes: vec![Scene::new("Scene 1")],
            assets: Vec::new(),
            start_slide: None,
            lms: LmsSettings::default(),
            player: PlayerSettings::default(),
        }
    }

    /// All slides in course order, with their location.
    pub fn slides(&self) -> impl Iterator<Item = (SlideLocation, &Slide)> {
        self.scenes.iter().enumerate().flat_map(|(scene, s)| {
            s.slides.iter().enumerate().map(move |(slide, sl)| (SlideLocation { scene, slide }, sl))
        })
    }

    pub fn slide_count(&self) -> usize {
        self.scenes.iter().map(|s| s.slides.len()).sum()
    }

    pub fn locate_slide(&self, id: &Id) -> Option<SlideLocation> {
        self.slides().find(|(_, s)| &s.id == id).map(|(loc, _)| loc)
    }

    pub fn slide(&self, id: &Id) -> Option<&Slide> {
        self.slides().find(|(_, s)| &s.id == id).map(|(_, s)| s)
    }

    pub fn slide_at(&self, loc: SlideLocation) -> Option<&Slide> {
        self.scenes.get(loc.scene)?.slides.get(loc.slide)
    }

    pub fn slide_at_mut(&mut self, loc: SlideLocation) -> Option<&mut Slide> {
        self.scenes.get_mut(loc.scene)?.slides.get_mut(loc.slide)
    }

    /// The slide the course opens on.
    pub fn start_slide_id(&self) -> Option<&Id> {
        self.start_slide
            .as_ref()
            .filter(|id| self.slide(id).is_some())
            .or_else(|| self.slides().next().map(|(_, s)| &s.id))
    }

    pub fn variable(&self, id: &Id) -> Option<&Variable> {
        self.variables.iter().find(|v| &v.id == id)
    }

    pub fn variable_by_name(&self, name: &str) -> Option<&Variable> {
        self.variables.iter().find(|v| v.name == name)
    }

    pub fn asset(&self, id: &Id) -> Option<&Asset> {
        self.assets.iter().find(|a| &a.id == id)
    }

    /// Find an object anywhere in the project.
    pub fn find_object(&self, id: &Id) -> Option<(&Slide, &Layer, &Object)> {
        self.slides().find_map(|(_, slide)| {
            slide.layers().find_map(|layer| layer.objects.iter().find(|o| &o.id == id).map(|o| (slide, layer, o)))
        })
    }

    /// Check references between items and return a human-readable list of
    /// problems. An empty list means the project is consistent.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.slide_count() == 0 {
            problems.push("project has no slides".to_owned());
        }
        for (_, slide) in self.slides() {
            for layer in slide.layers() {
                let at = format!("slide {:?}, layer {:?}", slide.name, layer.name);
                for object in &layer.objects {
                    if let crate::ObjectKind::Image { asset } = &object.kind {
                        if self.asset(asset).is_none() {
                            problems.push(format!("{at}: image {:?} uses missing asset {asset}", object.name));
                        }
                    }
                }
                for trigger in &layer.triggers {
                    self.validate_trigger(slide, trigger, &at, &mut problems);
                }
            }
        }
        problems
    }

    fn validate_trigger(&self, slide: &Slide, trigger: &Trigger, at: &str, problems: &mut Vec<String>) {
        use crate::{Action, Condition, Event, LayerTarget, Operand, SlideTarget};

        let check_object = |id: &Id, problems: &mut Vec<String>| {
            if slide.find_object(id).is_none() {
                problems.push(format!("{at}: trigger {} refers to object {id} not on this slide", trigger.id));
            }
        };
        let check_var = |id: &Id, problems: &mut Vec<String>| {
            if self.variable(id).is_none() {
                problems.push(format!("{at}: trigger {} refers to missing variable {id}", trigger.id));
            }
        };
        let check_layer = |t: &LayerTarget, problems: &mut Vec<String>| {
            if let LayerTarget::Layer(id) = t {
                if slide.layer(id).is_none() {
                    problems.push(format!("{at}: trigger {} refers to missing layer {id}", trigger.id));
                }
            }
        };

        match &trigger.event {
            Event::Click { object } | Event::HoverStart { object } | Event::HoverEnd { object } => {
                check_object(object, problems)
            }
            Event::VariableChanges { variable } => check_var(variable, problems),
            Event::TimelineStarts | Event::TimelineEnds | Event::KeyPressed { .. } => {}
        }
        match &trigger.action {
            Action::JumpToSlide { target: SlideTarget::Slide(id) } if self.slide(id).is_none() => {
                problems.push(format!("{at}: trigger {} jumps to missing slide {id}", trigger.id));
            }
            Action::ShowLayer { layer } => check_layer(&LayerTarget::Layer(layer.clone()), problems),
            Action::HideLayer { layer }
            | Action::PauseTimeline { layer }
            | Action::ResumeTimeline { layer }
            | Action::RestartTimeline { layer } => check_layer(layer, problems),
            Action::ChangeState { object, .. } => check_object(object, problems),
            Action::SetVariable { variable, value, .. } => {
                check_var(variable, problems);
                if let Operand::Variable(v) = value {
                    check_var(v, problems);
                }
            }
            _ => {}
        }
        for clause in &trigger.conditions {
            match &clause.condition {
                Condition::Variable { variable, value, .. } => {
                    check_var(variable, problems);
                    if let Operand::Variable(v) = value {
                        check_var(v, problems);
                    }
                }
                Condition::ObjectState { object, .. } => check_object(object, problems),
            }
        }
    }
}

/// Position of a slide in the scene list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlideLocation {
    pub scene: usize,
    pub slide: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub id: Id,
    pub name: String,
    pub slides: Vec<Slide>,
}

impl Scene {
    pub fn new(name: impl Into<String>) -> Self {
        Scene { id: Id::new(), name: name.into(), slides: vec![Slide::new("Untitled Slide")] }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    pub id: Id,
    pub name: String,
    pub background: Color,
    #[serde(default)]
    pub advance: Advance,
    pub base: Layer,
    /// Additional layers, bottom to top.
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl Slide {
    pub fn new(name: impl Into<String>) -> Self {
        Slide {
            id: Id::new(),
            name: name.into(),
            background: Color::WHITE,
            advance: Advance::default(),
            base: Layer::new("Base Layer"),
            layers: Vec::new(),
            notes: String::new(),
        }
    }

    /// The base layer followed by the other layers, bottom to top.
    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        std::iter::once(&self.base).chain(self.layers.iter())
    }

    pub fn layer(&self, id: &Id) -> Option<&Layer> {
        self.layers().find(|l| &l.id == id)
    }

    pub fn layer_mut(&mut self, id: &Id) -> Option<&mut Layer> {
        if &self.base.id == id {
            return Some(&mut self.base);
        }
        self.layers.iter_mut().find(|l| &l.id == id)
    }

    pub fn find_object(&self, id: &Id) -> Option<(&Layer, &Object)> {
        self.layers().find_map(|l| l.objects.iter().find(|o| &o.id == id).map(|o| (l, o)))
    }

    pub fn find_object_mut(&mut self, id: &Id) -> Option<&mut Object> {
        std::iter::once(&mut self.base)
            .chain(self.layers.iter_mut())
            .find_map(|l| l.objects.iter_mut().find(|o| &o.id == id))
    }
}

/// What happens when the base layer's timeline ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Advance {
    /// Go to the next slide.
    Automatically,
    /// Wait for the learner (a button, a trigger or the player's Next).
    #[default]
    ByUser,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: Id,
    pub name: String,
    /// Bottom to top drawing order.
    #[serde(default)]
    pub objects: Vec<Object>,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    pub duration_ms: u32,
    /// When shown, hide every other non-base layer. Ignored on base layers.
    #[serde(default)]
    pub hide_other_layers: bool,
    /// When shown, pause the base layer timeline until this layer is hidden.
    #[serde(default)]
    pub pause_base_timeline: bool,
    /// While shown, objects on the base layer cannot be clicked or hovered.
    #[serde(default)]
    pub prevent_base_clicks: bool,
    /// Hide this layer automatically when its timeline ends.
    #[serde(default)]
    pub hide_when_timeline_ends: bool,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Layer {
            id: Id::new(),
            name: name.into(),
            objects: Vec::new(),
            triggers: Vec::new(),
            duration_ms: 5000,
            hide_other_layers: true,
            pause_base_timeline: false,
            prevent_base_clicks: false,
            hide_when_timeline_ends: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: Id,
    /// Display name, usually the original file name.
    pub name: String,
    pub mime: String,
    /// File name inside the archive's `media/` folder and in published output.
    pub file_name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct LmsSettings {
    /// Identifier reported to the LMS. Defaults to the project id when empty.
    #[serde(default)]
    pub course_id: String,
    #[serde(default)]
    pub completion: Completion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Completion {
    /// Complete once every slide has been viewed.
    #[default]
    AllSlidesViewed,
    /// Complete once at least `count` distinct slides have been viewed.
    SlidesViewed { count: u32 },
    /// Complete only when a Complete course trigger runs.
    OnTrigger,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerSettings {
    /// Show the player's Previous/Next buttons under the slide.
    #[serde(default = "yes")]
    pub show_nav_buttons: bool,
    /// Colour around the slide when the window is larger than the slide.
    #[serde(default = "default_backdrop")]
    pub backdrop: Color,
}

fn yes() -> bool {
    true
}

fn default_backdrop() -> Color {
    Color::rgb(0x22, 0x22, 0x22)
}

impl Default for PlayerSettings {
    fn default() -> Self {
        PlayerSettings { show_nav_buttons: true, backdrop: default_backdrop() }
    }
}
