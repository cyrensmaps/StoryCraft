//! Application state, menus, file handling and dialogs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Key, KeyboardShortcut, Modifiers, ViewportCommand};
use storycraft_export::Target;
use storycraft_model::archive::{self, Document};
use storycraft_model::{
    Id, Layer, Object, ObjectKind, Project, Rect, Scene, ShapeKind, Slide, SlideLocation, TextContent,
    sample::sample_project,
};

use crate::canvas::CanvasState;
use crate::history::History;
use crate::preview::Preview;
use crate::textures::{Textures, image_size};
use crate::{canvas, panels};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RightTab {
    Properties,
    Triggers,
    Variables,
}

/// What is being edited.
pub struct Selection {
    pub slide: SlideLocation,
    /// The layer being edited on the current slide.
    pub layer: Id,
    /// Selected objects on that layer; the first is the primary selection.
    pub objects: Vec<Id>,
}

/// Actions that replace or leave the document, which may need a "save
/// changes?" prompt first.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leave {
    New,
    NewSample,
    Open,
    ImportStory,
    Exit,
}

/// Commands that must run outside the UI pass so they are not recorded as
/// edits (undo/redo) or because they replace the whole document.
#[derive(Clone)]
enum Command {
    Undo,
    Redo,
    Leave(Leave),
    Save,
    SaveAs,
    Publish(Target),
    Preview { from_current: bool },
    PreviewInBrowser,
}

enum Dialog {
    Error(String),
    Published { path: PathBuf, open: Option<PathBuf>, warnings: Vec<String> },
    ConfirmLeave(Leave),
    About,
}

pub struct EditorApp {
    pub doc: Document,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub history: History,
    pub sel: Selection,
    pub textures: Textures,
    pub preview: Option<Preview>,
    pub right_tab: RightTab,
    pub clipboard: Vec<Object>,
    pub canvas: CanvasState,
    pub status: String,
    commands: Vec<Command>,
    dialog: Option<Dialog>,
    allow_close: bool,
    title: String,
}

impl EditorApp {
    pub fn new(initial: Option<PathBuf>) -> Self {
        let doc = Document::new(Project::new("Untitled Course"));
        let mut app = EditorApp {
            sel: Selection {
                slide: SlideLocation { scene: 0, slide: 0 },
                layer: doc.project.scenes[0].slides[0].base.id.clone(),
                objects: Vec::new(),
            },
            doc,
            path: None,
            dirty: false,
            history: History::default(),
            textures: Textures::default(),
            preview: None,
            right_tab: RightTab::Properties,
            clipboard: Vec::new(),
            canvas: CanvasState::default(),
            status: "Ready".into(),
            commands: Vec::new(),
            dialog: None,
            allow_close: false,
            title: String::new(),
        };
        if let Some(path) = initial {
            app.open_path(&path);
        }
        app
    }

    // --- document access ---------------------------------------------------

    pub fn project(&self) -> &Project {
        &self.doc.project
    }

    pub fn slide(&self) -> &Slide {
        self.doc.project.slide_at(self.sel.slide).expect("selection is kept valid")
    }

    pub fn slide_mut(&mut self) -> &mut Slide {
        self.doc.project.slide_at_mut(self.sel.slide).expect("selection is kept valid")
    }

    pub fn layer(&self) -> &Layer {
        self.slide().layer(&self.sel.layer).unwrap_or(&self.slide().base)
    }

    pub fn layer_mut(&mut self) -> &mut Layer {
        let id = self.sel.layer.clone();
        let slide = self.slide_mut();
        if slide.layer(&id).is_some() { slide.layer_mut(&id).unwrap() } else { &mut slide.base }
    }

    pub fn is_base_layer(&self) -> bool {
        self.sel.layer == self.slide().base.id
    }

    /// The primary selected object.
    pub fn selected(&self) -> Option<&Object> {
        let id = self.sel.objects.first()?;
        self.layer().objects.iter().find(|o| &o.id == id)
    }

    pub fn selected_mut(&mut self) -> Option<&mut Object> {
        let id = self.sel.objects.first()?.clone();
        self.layer_mut().objects.iter_mut().find(|o| o.id == id)
    }

    pub fn select_slide(&mut self, loc: SlideLocation) {
        if let Some(slide) = self.doc.project.slide_at(loc) {
            self.sel = Selection { slide: loc, layer: slide.base.id.clone(), objects: Vec::new() };
        }
    }

    pub fn select_layer(&mut self, id: Id) {
        self.sel.layer = id;
        self.sel.objects.clear();
    }

    /// Repair the selection after undo, deletion and so on.
    fn sanitize_selection(&mut self) {
        let p = &self.doc.project;
        if p.slide_at(self.sel.slide).is_none() {
            let fallback = p.slides().next().map(|(loc, _)| loc);
            match fallback {
                Some(loc) => self.select_slide(loc),
                None => {
                    // Never leave the project without a slide to edit.
                    if self.doc.project.scenes.is_empty() {
                        self.doc.project.scenes.push(Scene::new("Scene 1"));
                    } else {
                        self.doc.project.scenes[0].slides.push(Slide::new("Untitled Slide"));
                    }
                    self.select_slide(SlideLocation { scene: 0, slide: 0 });
                }
            }
        }
        if self.slide().layer(&self.sel.layer).is_none() {
            self.sel.layer = self.slide().base.id.clone();
        }
        let layer = self.layer();
        let keep: Vec<Id> =
            self.sel.objects.iter().filter(|id| layer.objects.iter().any(|o| &o.id == *id)).cloned().collect();
        self.sel.objects = keep;
    }

    fn replace_document(&mut self, doc: Document, path: Option<PathBuf>) {
        self.doc = doc;
        self.path = path;
        self.dirty = false;
        self.history.clear();
        self.textures.clear();
        self.preview = None;
        self.sel.slide = SlideLocation { scene: usize::MAX, slide: 0 };
        self.sanitize_selection();
    }

    // --- editing actions (recorded by the history) --------------------------

    /// Start a fresh undo step for a discrete action.
    fn begin_action(&mut self) {
        self.history.commit();
    }

    pub fn insert_object(&mut self, mut object: Object) {
        self.begin_action();
        let size = self.doc.project.slide_size;
        object.frame.x = ((size.width - object.frame.width) / 2.0).round();
        object.frame.y = ((size.height - object.frame.height) / 2.0).round();
        self.sel.objects = vec![object.id.clone()];
        self.layer_mut().objects.push(object);
        self.right_tab = RightTab::Properties;
    }

    pub fn insert_text(&mut self) {
        let mut o = Object::new("Text Box", ObjectKind::Text, Rect::new(0.0, 0.0, 400.0, 80.0));
        o.text = Some(TextContent::new("Type your text here"));
        self.insert_object(o);
    }

    pub fn insert_shape(&mut self, shape: ShapeKind) {
        let mut o = Object::new(shape.label(), ObjectKind::Shape { shape }, Rect::new(0.0, 0.0, 200.0, 150.0));
        o.style.fill = Some(storycraft_model::Color::rgb(0x5B, 0x9B, 0xD5));
        self.insert_object(o);
    }

    pub fn insert_button(&mut self) {
        let mut o = Object::new("Button", ObjectKind::Button, Rect::new(0.0, 0.0, 200.0, 60.0));
        o.style.fill = Some(storycraft_model::Color::rgb(0x2B, 0x6C, 0xB0));
        o.style.corner_radius = 8.0;
        let mut t = TextContent::new("Button");
        t.color = storycraft_model::Color::WHITE;
        o.text = Some(t);
        o.states = vec![
            storycraft_model::ObjectState {
                name: storycraft_model::state_names::HOVER.into(),
                overrides: storycraft_model::StateOverrides {
                    fill: Some(storycraft_model::Color::rgb(0x3A, 0x82, 0xCF)),
                    ..Default::default()
                },
            },
            storycraft_model::ObjectState {
                name: storycraft_model::state_names::DOWN.into(),
                overrides: storycraft_model::StateOverrides {
                    fill: Some(storycraft_model::Color::rgb(0x1E, 0x4E, 0x80)),
                    ..Default::default()
                },
            },
        ];
        self.insert_object(o);
    }

    pub fn insert_web_address(&mut self) {
        let source = storycraft_model::WebSource::Url { url: "https://".into() };
        self.insert_web(source, "Web Object".into());
    }

    /// Bundle a folder of web content (it needs an HTML page) into the project.
    pub fn insert_web_folder(&mut self) {
        let Some(dir) = rfd::FileDialog::new().set_title("Choose a folder with an index.html").pick_folder() else {
            return;
        };
        let bundle = match archive::bundle_folder(&dir) {
            Ok(b) => b,
            Err(e) => return self.error(format!("Could not read {}:\n\n{e}", dir.display())),
        };
        let paths: Vec<String> =
            archive::bundle_files(&bundle).map(|f| f.into_iter().map(|(p, _)| p).collect()).unwrap_or_default();
        let Some(entry) = archive::bundle_start_page(&paths, Some("index.html")) else {
            return self.error(format!("{} has no web page (.html file) in it.", dir.display()));
        };
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "web".into());
        self.begin_action();
        let asset = self.doc.add_asset(&format!("{name}.zip"), "application/zip", bundle);
        self.insert_web(storycraft_model::WebSource::Bundle { asset, entry }, name);
    }

    fn insert_web(&mut self, source: storycraft_model::WebSource, name: String) {
        let size = self.doc.project.slide_size;
        let frame = Rect::new(0.0, 0.0, (size.width * 0.8).round(), (size.height * 0.8).round());
        let kind = ObjectKind::Web { source, display: storycraft_model::WebDisplay::OnSlide };
        self.insert_object(Object::new(name, kind, frame));
    }

    /// Publish to a temporary folder and open it in the system browser, which
    /// can show what the editor cannot (web objects, real fonts).
    fn preview_in_browser(&mut self) {
        let doc = prune_media(&self.doc);
        let package = match storycraft_export::build(&doc, Target::Html) {
            Ok(p) => p,
            Err(e) => return self.error(format!("Preview failed:\n\n{e}")),
        };
        let dir = std::env::temp_dir().join("storycraft-preview").join(self.doc.project.id.as_str());
        let _ = std::fs::remove_dir_all(&dir);
        match package.write_dir(&dir) {
            Ok(()) => {
                open_in_system(&dir.join("index.html"));
                self.status = "Opened the preview in your browser".into();
            }
            Err(e) => self.error(format!("Preview failed:\n\n{e}")),
        }
    }

    pub fn insert_image(&mut self) {
        let Some(path) =
            rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp"]).pick_file()
        else {
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => return self.error(format!("Could not read {}: {e}", path.display())),
        };
        let Some((w, h)) = image_size(&bytes) else {
            return self.error(format!("{} is not an image StoryCraft can read.", path.display()));
        };
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
        let slide = self.doc.project.slide_size;
        let fit = (slide.width * 0.6 / w as f32).min(slide.height * 0.6 / h as f32).min(1.0);
        self.begin_action();
        let asset = self.doc.add_asset(&name, archive::mime_for(&name), bytes);
        let mut o = Object::new(
            name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem),
            ObjectKind::Image { asset },
            Rect::new(0.0, 0.0, (w as f32 * fit).round(), (h as f32 * fit).round()),
        );
        o.alt_text = String::new();
        self.insert_object(o);
    }

    pub fn delete_selected(&mut self) {
        if self.sel.objects.is_empty() {
            return;
        }
        self.begin_action();
        let ids = std::mem::take(&mut self.sel.objects);
        self.layer_mut().objects.retain(|o| !ids.contains(&o.id));
    }

    pub fn copy_selected(&mut self) {
        let layer = self.layer();
        self.clipboard = layer.objects.iter().filter(|o| self.sel.objects.contains(&o.id)).cloned().collect();
        if !self.clipboard.is_empty() {
            self.status = format!("Copied {} object(s)", self.clipboard.len());
        }
    }

    /// Paste copies offset slightly, so they are visibly new.
    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        self.begin_action();
        let copies: Vec<Object> = self
            .clipboard
            .iter()
            .map(|o| {
                let mut c = o.clone();
                c.id = Id::new();
                c.frame.x += 20.0;
                c.frame.y += 20.0;
                c
            })
            .collect();
        self.sel.objects = copies.iter().map(|o| o.id.clone()).collect();
        self.clipboard = copies.clone();
        self.layer_mut().objects.extend(copies);
    }

    pub fn duplicate_selected(&mut self) {
        self.copy_selected();
        self.paste();
    }

    /// Move the selection one step up (`up`) or down the stacking order.
    pub fn reorder_selected(&mut self, up: bool) {
        let Some(id) = self.sel.objects.first().cloned() else { return };
        self.begin_action();
        let objects = &mut self.layer_mut().objects;
        if let Some(i) = objects.iter().position(|o| o.id == id) {
            let j = if up { i + 1 } else { i.wrapping_sub(1) };
            if j < objects.len() {
                objects.swap(i, j);
            }
        }
    }

    pub fn add_slide(&mut self) {
        self.begin_action();
        let loc = self.sel.slide;
        let scene = &mut self.doc.project.scenes[loc.scene];
        let n = scene.slides.len() + 1;
        let mut slide = Slide::new(format!("Slide {n}"));
        slide.background = scene.slides[loc.slide].background;
        scene.slides.insert(loc.slide + 1, slide);
        self.select_slide(SlideLocation { scene: loc.scene, slide: loc.slide + 1 });
    }

    pub fn add_scene(&mut self) {
        self.begin_action();
        let n = self.doc.project.scenes.len() + 1;
        self.doc.project.scenes.push(Scene::new(format!("Scene {n}")));
        self.select_slide(SlideLocation { scene: n - 1, slide: 0 });
    }

    pub fn duplicate_slide(&mut self, loc: SlideLocation) {
        let Some(slide) = self.doc.project.slide_at(loc).cloned() else { return };
        self.begin_action();
        let copy = fresh_slide_ids(slide);
        self.doc.project.scenes[loc.scene].slides.insert(loc.slide + 1, copy);
        self.select_slide(SlideLocation { scene: loc.scene, slide: loc.slide + 1 });
    }

    pub fn delete_slide(&mut self, loc: SlideLocation) {
        if self.doc.project.slide_count() <= 1 {
            self.status = "A course needs at least one slide.".into();
            return;
        }
        self.begin_action();
        let scenes = &mut self.doc.project.scenes;
        scenes[loc.scene].slides.remove(loc.slide);
        if scenes[loc.scene].slides.is_empty() {
            scenes.remove(loc.scene);
        }
        let fallback = SlideLocation { scene: loc.scene.min(scenes.len() - 1), slide: loc.slide.saturating_sub(1) };
        let fallback = if self.doc.project.slide_at(fallback).is_some() {
            fallback
        } else {
            SlideLocation { scene: fallback.scene, slide: 0 }
        };
        self.select_slide(fallback);
    }

    pub fn move_slide(&mut self, loc: SlideLocation, up: bool) {
        let slides = &mut self.doc.project.scenes[loc.scene].slides;
        let target = if up { loc.slide.wrapping_sub(1) } else { loc.slide + 1 };
        if target < slides.len() {
            self.begin_action();
            self.doc.project.scenes[loc.scene].slides.swap(loc.slide, target);
            self.select_slide(SlideLocation { scene: loc.scene, slide: target });
        }
    }

    pub fn add_layer(&mut self) {
        self.begin_action();
        let n = self.slide().layers.len() + 1;
        let mut layer = Layer::new(format!("Layer {n}"));
        layer.duration_ms = self.slide().base.duration_ms;
        let id = layer.id.clone();
        self.slide_mut().layers.push(layer);
        self.select_layer(id);
    }

    pub fn delete_layer(&mut self, id: &Id) {
        self.begin_action();
        self.slide_mut().layers.retain(|l| &l.id != id);
    }

    pub fn duplicate_layer(&mut self, id: &Id) {
        let Some(layer) = self.slide().layers.iter().find(|l| &l.id == id).cloned() else { return };
        self.begin_action();
        let mut copy = layer;
        copy.id = Id::new();
        copy.name = format!("{} copy", copy.name);
        for o in &mut copy.objects {
            o.id = Id::new();
        }
        let new_id = copy.id.clone();
        self.slide_mut().layers.push(copy);
        self.select_layer(new_id);
    }

    // --- commands ------------------------------------------------------------

    fn queue(&mut self, ctx: &egui::Context, command: Command) {
        self.commands.push(command);
        ctx.request_repaint();
    }

    fn run_commands(&mut self, ctx: &egui::Context) {
        for command in std::mem::take(&mut self.commands) {
            match command {
                Command::Undo => {
                    self.history.undo(&mut self.doc.project);
                    self.dirty = true;
                }
                Command::Redo => {
                    self.history.redo(&mut self.doc.project);
                    self.dirty = true;
                }
                Command::Leave(leave) => {
                    if self.dirty {
                        self.dialog = Some(Dialog::ConfirmLeave(leave));
                    } else {
                        self.leave(ctx, leave);
                    }
                }
                Command::Save => {
                    self.save();
                }
                Command::SaveAs => {
                    self.save_as();
                }
                Command::Publish(target) => self.publish(target),
                Command::PreviewInBrowser => self.preview_in_browser(),
                Command::Preview { from_current } => {
                    let from = from_current.then(|| self.slide().id.clone());
                    match Preview::new(&self.doc.project, from.as_ref()) {
                        Ok(p) => self.preview = Some(p),
                        Err(e) => self.error(e),
                    }
                }
            }
            self.sanitize_selection();
        }
    }

    fn leave(&mut self, ctx: &egui::Context, leave: Leave) {
        match leave {
            Leave::New => self.replace_document(Document::new(Project::new("Untitled Course")), None),
            Leave::NewSample => self.replace_document(Document::new(sample_project()), None),
            Leave::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("StoryCraft project", &[archive::EXTENSION])
                    .add_filter("Storyline project", &["story"])
                    .pick_file()
                {
                    self.open_path(&path);
                }
            }
            Leave::ImportStory => {
                if let Some(path) = rfd::FileDialog::new().add_filter("Storyline project", &["story"]).pick_file() {
                    self.open_path(&path);
                }
            }
            Leave::Exit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    pub fn open_path(&mut self, path: &Path) {
        let is_story = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("story"));
        if is_story {
            match storycraft_story::import(path) {
                Ok(imported) => {
                    // Imported projects are saved as new .storycraft files.
                    self.replace_document(imported.document, None);
                    self.dirty = true;
                    self.status = format!("Imported {}", path.display());
                    if !imported.warnings.is_empty() {
                        self.error(format!("Imported with notes:\n\n{}", imported.warnings.join("\n")));
                    }
                }
                Err(e) => self.error(format!("Could not import {}:\n\n{e}", path.display())),
            }
        } else {
            match Document::load(path) {
                Ok(doc) => {
                    self.replace_document(doc, Some(path.to_owned()));
                    self.status = format!("Opened {}", path.display());
                }
                Err(e) => self.error(format!("Could not open {}:\n\n{e}", path.display())),
            }
        }
    }

    fn save(&mut self) -> bool {
        match self.path.clone() {
            Some(path) => self.save_to(&path),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let name = format!("{}.{}", sanitize_file_name(&self.doc.project.title), archive::EXTENSION);
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter("StoryCraft project", &[archive::EXTENSION])
            .set_file_name(name)
            .save_file()
        else {
            return false;
        };
        if path.extension().is_none() {
            path.set_extension(archive::EXTENSION);
        }
        self.save_to(&path)
    }

    fn save_to(&mut self, path: &Path) -> bool {
        self.history.commit();
        // Drop media no longer referenced by any asset (e.g. after undo).
        let doc = prune_media(&self.doc);
        match doc.save(path) {
            Ok(()) => {
                self.path = Some(path.to_owned());
                self.dirty = false;
                self.status = format!("Saved {}", path.display());
                true
            }
            Err(e) => {
                self.error(format!("Could not save {}:\n\n{e}", path.display()));
                false
            }
        }
    }

    fn publish(&mut self, target: Target) {
        let doc = prune_media(&self.doc);
        let package = match storycraft_export::build(&doc, target) {
            Ok(p) => p,
            Err(e) => return self.error(format!("Publishing failed:\n\n{e}")),
        };
        let stem = sanitize_file_name(&self.doc.project.title);
        let (out, open) = match target {
            Target::Html => {
                let Some(dir) =
                    rfd::FileDialog::new().set_title("Choose an empty folder for the website").pick_folder()
                else {
                    return;
                };
                let index = dir.join("index.html");
                (dir, Some(index))
            }
            _ => {
                let Some(mut file) = rfd::FileDialog::new()
                    .add_filter("LMS package", &["zip"])
                    .set_file_name(format!("{stem}-{}.zip", target.id()))
                    .save_file()
                else {
                    return;
                };
                if file.extension().is_none() {
                    file.set_extension("zip");
                }
                (file, None)
            }
        };
        match package.write_to(&out) {
            Ok(()) => {
                self.status = format!("Published to {}", out.display());
                self.dialog = Some(Dialog::Published { path: out, open, warnings: package.warnings });
            }
            Err(e) => self.error(format!("Could not write {}:\n\n{e}", out.display())),
        }
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.dialog = Some(Dialog::Error(message.into()));
    }

    // --- frame -----------------------------------------------------------------

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = |key| KeyboardShortcut::new(Modifiers::COMMAND, key);
        let cmd_shift = |key| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, key);
        let pressed = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));

        if pressed(cmd_shift(Key::Z)) || pressed(cmd(Key::Y)) {
            self.queue(ctx, Command::Redo);
        } else if pressed(cmd(Key::Z)) {
            self.queue(ctx, Command::Undo);
        }
        if pressed(cmd_shift(Key::S)) {
            self.queue(ctx, Command::SaveAs);
        } else if pressed(cmd(Key::S)) {
            self.queue(ctx, Command::Save);
        }
        if pressed(cmd(Key::O)) {
            self.queue(ctx, Command::Leave(Leave::Open));
        }
        if pressed(cmd(Key::N)) {
            self.queue(ctx, Command::Leave(Leave::New));
        }
        if pressed(KeyboardShortcut::new(Modifiers::NONE, Key::F12)) {
            self.queue(ctx, Command::Preview { from_current: false });
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.queue(&ctx, Command::Leave(Leave::New));
                }
                if ui.button("New from sample").clicked() {
                    self.queue(&ctx, Command::Leave(Leave::NewSample));
                }
                if ui.button("Open…").clicked() {
                    self.queue(&ctx, Command::Leave(Leave::Open));
                }
                if ui.button("Import Storyline project…").clicked() {
                    self.queue(&ctx, Command::Leave(Leave::ImportStory));
                }
                ui.separator();
                if ui.button("Save").clicked() {
                    self.queue(&ctx, Command::Save);
                }
                if ui.button("Save as…").clicked() {
                    self.queue(&ctx, Command::SaveAs);
                }
                ui.separator();
                ui.menu_button("Publish", |ui| {
                    for target in Target::ALL {
                        if ui.button(target.label()).clicked() {
                            self.queue(&ctx, Command::Publish(target));
                        }
                    }
                });
                ui.separator();
                if ui.button("Exit").clicked() {
                    self.queue(&ctx, Command::Leave(Leave::Exit));
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(self.history.can_undo(), egui::Button::new("Undo")).clicked() {
                    self.queue(&ctx, Command::Undo);
                }
                if ui.add_enabled(self.history.can_redo(), egui::Button::new("Redo")).clicked() {
                    self.queue(&ctx, Command::Redo);
                }
                ui.separator();
                let has_sel = !self.sel.objects.is_empty();
                if ui.add_enabled(has_sel, egui::Button::new("Copy")).clicked() {
                    self.copy_selected();
                }
                if ui.add_enabled(!self.clipboard.is_empty(), egui::Button::new("Paste")).clicked() {
                    self.paste();
                }
                if ui.add_enabled(has_sel, egui::Button::new("Duplicate")).clicked() {
                    self.duplicate_selected();
                }
                if ui.add_enabled(has_sel, egui::Button::new("Delete")).clicked() {
                    self.delete_selected();
                }
                ui.separator();
                if ui.add_enabled(has_sel, egui::Button::new("Bring forward")).clicked() {
                    self.reorder_selected(true);
                }
                if ui.add_enabled(has_sel, egui::Button::new("Send backward")).clicked() {
                    self.reorder_selected(false);
                }
            });
            ui.menu_button("Insert", |ui| self.insert_menu(ui));
            ui.menu_button("Slides", |ui| {
                if ui.button("New slide").clicked() {
                    self.add_slide();
                }
                if ui.button("Duplicate slide").clicked() {
                    self.duplicate_slide(self.sel.slide);
                }
                if ui.button("Delete slide").clicked() {
                    self.delete_slide(self.sel.slide);
                }
                ui.separator();
                if ui.button("New scene").clicked() {
                    self.add_scene();
                }
                if ui.button("New layer").clicked() {
                    self.add_layer();
                }
            });
            ui.menu_button("Preview", |ui| {
                if ui.button("Preview course (F12)").clicked() {
                    self.queue(&ctx, Command::Preview { from_current: false });
                }
                if ui.button("Preview from this slide").clicked() {
                    self.queue(&ctx, Command::Preview { from_current: true });
                }
                ui.separator();
                if ui
                    .button("Preview in browser")
                    .on_hover_text("Shows web objects and fonts as learners see them")
                    .clicked()
                {
                    self.queue(&ctx, Command::PreviewInBrowser);
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About StoryCraft").clicked() {
                    self.dialog = Some(Dialog::About);
                }
            });
        });
    }

    pub fn insert_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button("Text box").clicked() {
            self.insert_text();
        }
        if ui.button("Button").clicked() {
            self.insert_button();
        }
        ui.separator();
        for shape in ShapeKind::ALL {
            if ui.button(shape.label()).clicked() {
                self.insert_shape(shape);
            }
        }
        ui.separator();
        if ui.button("Picture…").clicked() {
            self.insert_image();
        }
        ui.separator();
        if ui.button("Web object (address)").clicked() {
            self.insert_web_address();
        }
        if ui.button("Web object (folder)…").clicked() {
            self.insert_web_folder();
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            if ui.button("➕ Slide").on_hover_text("New slide").clicked() {
                self.add_slide();
            }
            ui.separator();
            if ui.button("T  Text").clicked() {
                self.insert_text();
            }
            if ui.button("⬜ Shape").clicked() {
                self.insert_shape(ShapeKind::Rectangle);
            }
            if ui.button("🔘 Button").clicked() {
                self.insert_button();
            }
            if ui.button("🖼 Picture").clicked() {
                self.insert_image();
            }
            ui.separator();
            if ui
                .add_enabled(self.history.can_undo(), egui::Button::new("Undo"))
                .on_hover_text("Undo (Ctrl+Z)")
                .clicked()
            {
                self.queue(&ctx, Command::Undo);
            }
            if ui
                .add_enabled(self.history.can_redo(), egui::Button::new("Redo"))
                .on_hover_text("Redo (Ctrl+Y)")
                .clicked()
            {
                self.queue(&ctx, Command::Redo);
            }
            ui.separator();
            if ui.button("▶ Preview").on_hover_text("Preview the course (F12)").clicked() {
                self.queue(&ctx, Command::Preview { from_current: false });
            }
            if ui.button("▶ This slide").on_hover_text("Preview from the current slide").clicked() {
                self.queue(&ctx, Command::Preview { from_current: true });
            }
            ui.separator();
            ui.menu_button("📦 Publish", |ui| {
                for target in Target::ALL {
                    if ui.button(target.label()).clicked() {
                        self.queue(&ctx, Command::Publish(target));
                    }
                }
            });
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &self.dialog else { return };
        let mut close = false;
        let mut then: Option<Leave> = None;
        let mut save_first = false;
        egui::Modal::new(egui::Id::new("dialog")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            match dialog {
                Dialog::Error(message) => {
                    ui.heading("Something went wrong");
                    ui.add_space(6.0);
                    ui.label(message);
                    ui.add_space(10.0);
                    close = ui.button("OK").clicked();
                }
                Dialog::Published { path, open, warnings } => {
                    ui.heading("Published");
                    ui.label(format!("Your course was written to {}", path.display()));
                    if !warnings.is_empty() {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("Things to check:").strong());
                        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                            for w in warnings {
                                ui.label(format!("• {w}"));
                            }
                        });
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if let Some(index) = open {
                            if ui.button("Open in browser").clicked() {
                                open_in_system(index);
                                close = true;
                            }
                        }
                        if ui.button("Show folder").clicked() {
                            let folder = if path.is_dir() { path.clone() } else { path.parent().unwrap_or(path).to_owned() };
                            open_in_system(&folder);
                        }
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::ConfirmLeave(leave) => {
                    ui.heading("Save changes?");
                    ui.label("Your course has unsaved changes.");
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            save_first = true;
                            then = Some(*leave);
                        }
                        if ui.button("Don't save").clicked() {
                            then = Some(*leave);
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Dialog::About => {
                    ui.heading("StoryCraft");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.label("An open-source e-learning authoring tool, written in Rust.");
                    ui.hyperlink_to("Source code", env!("CARGO_PKG_REPOSITORY"));
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new(
                            "StoryCraft is an independent project. It is not affiliated with or endorsed by Articulate.",
                        )
                        .small(),
                    );
                    ui.add_space(10.0);
                    close = ui.button("Close").clicked();
                }
            }
        });
        if close {
            self.dialog = None;
        }
        if let Some(leave) = then {
            self.dialog = None;
            if !save_first || self.save() {
                self.leave(ctx, leave);
            }
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        let title = format!(
            "{}{} — StoryCraft",
            if self.dirty { "● " } else { "" },
            name.unwrap_or_else(|| self.doc.project.title.clone())
        );
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for EditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::ConfirmLeave(Leave::Exit));
        }

        self.run_commands(&ctx);
        let before = self.doc.project.clone();
        self.shortcuts(&ctx);

        egui::Panel::top("menu").show(ui, |ui| {
            self.menu_bar(ui);
            if self.preview.is_none() {
                self.toolbar(ui);
            }
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (scene, slide) = (self.sel.slide.scene + 1, self.sel.slide.slide + 1);
                    ui.label(format!("Scene {scene} · Slide {slide} · {} slides", self.doc.project.slide_count()));
                });
            });
        });

        if let Some(preview) = &mut self.preview {
            let mut closed = false;
            egui::CentralPanel::default().show(ui, |ui| {
                closed = preview.ui(ui, &mut self.textures, &self.doc.media);
            });
            if closed {
                self.preview = None;
            }
        } else {
            egui::Panel::left("slides").default_size(230.0).show(ui, |ui| panels::slides::show(self, ui));
            egui::Panel::right("inspector").default_size(360.0).show(ui, |ui| panels::inspector(self, ui));
            egui::Panel::bottom("timeline")
                .resizable(true)
                .default_size(230.0)
                .show(ui, |ui| panels::timeline::show(self, ui));
            egui::CentralPanel::default().show(ui, |ui| canvas::show(self, ui));
        }

        self.dialogs(&ctx);

        let (pointer_down, time) = ctx.input(|i| (i.pointer.any_down(), i.time));
        if self.history.observe(before, &self.doc.project, pointer_down, time) {
            self.dirty = true;
        }
        if self.history.is_settling() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        self.sanitize_selection();
        self.update_title(&ctx);
    }
}

/// Give every item in a copied slide a new id, and rewrite the slide's
/// internal references (triggers, conditions) to match.
fn fresh_slide_ids(mut slide: Slide) -> Slide {
    let mut map = std::collections::HashMap::new();
    let mut renew = |id: &mut Id| {
        let new = Id::new();
        map.insert(id.clone(), new.clone());
        *id = new;
    };
    renew(&mut slide.id);
    renew(&mut slide.base.id);
    for o in &mut slide.base.objects {
        renew(&mut o.id);
    }
    for layer in &mut slide.layers {
        renew(&mut layer.id);
        for o in &mut layer.objects {
            renew(&mut o.id);
        }
    }
    slide.name = format!("{} copy", slide.name);
    // Point the copied triggers at the copied objects and layers.
    for layer in std::iter::once(&mut slide.base).chain(slide.layers.iter_mut()) {
        for trigger in &mut layer.triggers {
            let mut value = serde_json::to_value(&*trigger).expect("triggers serialise");
            remap_ids(&mut value, &map);
            *trigger = serde_json::from_value(value).expect("remapped trigger deserialises");
            trigger.id = Id::new();
        }
    }
    slide
}

/// Replace every string in `value` that is a key of `map`.
fn remap_ids(value: &mut serde_json::Value, map: &std::collections::HashMap<Id, Id>) {
    match value {
        serde_json::Value::String(s) => {
            if let Some(new) = map.get(&Id::from(s.as_str())) {
                *s = new.to_string();
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(|v| remap_ids(v, map)),
        serde_json::Value::Object(fields) => fields.values_mut().for_each(|v| remap_ids(v, map)),
        _ => {}
    }
}

fn prune_media(doc: &Document) -> Document {
    let mut out = Document::new(doc.project.clone());
    for asset in &doc.project.assets {
        if let Some(bytes) = doc.media.get(&asset.id) {
            out.media.insert(asset.id.clone(), bytes.clone());
        }
    }
    out
}

fn sanitize_file_name(s: &str) -> String {
    let cleaned: String =
        s.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' }).collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() { "course".into() } else { trimmed.to_owned() }
}

/// Open a file or folder with the operating system's default handler.
pub fn open_in_system(path: &Path) {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = result {
        eprintln!("could not open {}: {e}", path.display());
    }
}
