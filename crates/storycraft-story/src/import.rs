//! Mapping from the `.story` package structure to StoryCraft's model.
//!
//! What is known about the format, from inspecting sample projects:
//!
//! * The file is an OPC package (ZIP with `[Content_Types].xml` and `_rels/`).
//!   `_rels/.rels` points at the main part, `story/story.xml`.
//! * `story.xml` holds variables (`varLst/var`, `type="user"` for author-made
//!   ones), the slide size (`propLst/prop[@id=15]/sz`), media (`mediaLst`)
//!   and scenes (`sceneLst/scene/sldIdLst/sldId`). Each `sldId` is a
//!   relationship id resolved through `story/_rels/story.xml.rels`.
//! * A slide (`sld`) has `shapeLst` (objects), `trigLst` (slide triggers),
//!   `sldLayerLst/sldLayer` (layers, each with its own `shapeLst`/`trigLst`),
//!   `tmProps@min` (timeline length, ms) and `advMode`.
//! * Objects are elements named after their kind (`textBox`, `rect`,
//!   `roundRect`, `oval`, `triangle`, `pic`, `btn`, …) with `g` (id),
//!   `typeName` (author's name; `name` is the default), `loc` (l/t/r/b in
//!   slide pixels), timing in `tmCtxLst/*@start,dur,untilEnd`, fill in
//!   `bG/solidFill/clr`, text colour in `bG/foreClr`, text as an escaped XML
//!   document in `text` (`Document/Content/Block/Span@Text`), states in
//!   `stateLst/state` (each a full copy of the object), and their own
//!   `trigLst`.
//! * Text formatting is `Block/Style@Justification` and
//!   `Span/Style@FontFamily,FontSize,FontIsBold,FontIsItalic,ForegroundColor`.
//!   Unstyled text takes its font and size from the object's
//!   `resize@fontNm,fontSz` (12 by default). `+minor`/`+major` refer to the
//!   theme fonts in the master's `fonts@minor,major`.
//! * Layer options: `pause` (pause base timeline), `stop` (hide other slide
//!   layers), `modal` (prevent clicking on the base layer), `hideEnd` (hide
//!   the layer when its timeline finishes).
//! * Each condition's `andOr` joins it to the condition before it (the first
//!   condition's value is unused).
//! * Colours are `srgbClr@val` or `schemeClr@val`; scheme colours come from
//!   the `clrLst` of the master that owns the slide's layout (`layoutG`).
//! * Media files are matched by the MD5 checksum stored in `media/md5Checksum`.

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Seek};

use md5::{Digest, Md5};
use storycraft_model::archive::{Document, mime_for};
use storycraft_model::{
    Action, Advance, Color, Comparison, Condition, ConditionClause, Event, HAlign, Id, Join, Layer, LayerTarget,
    Object, ObjectKind, ObjectState, Operand, Project, Rect, Scene, ShapeKind, Size, Slide, SlideTarget,
    StateOverrides, Stroke, TextContent, Timing, Trigger, VAlign, Value, ValueKind, VarOp, Variable, state_names,
};

use crate::dom::{self, Node};
use crate::{ImportError, Imported};

const NIL_GUID: &str = "00000000-0000-0000-0000-000000000000";
/// Storyline's default text size, used when an object records none.
/// Unstyled text normally records it in `resize@fontSz`.
const DEFAULT_FONT_SIZE: f32 = 12.0;

/// A master's theme: its colour scheme and its major/minor fonts.
#[derive(Clone, Default)]
struct Palette {
    colors: HashMap<String, Color>,
    /// "major" / "minor" → font name.
    fonts: HashMap<String, String>,
}

impl Palette {
    fn get(&self, key: &str) -> Option<&Color> {
        self.colors.get(key)
    }

    /// Resolve a font name, including the theme references `+major`/`+minor`,
    /// to a CSS font-family value.
    fn font(&self, name: &str) -> Option<String> {
        let name = match name.strip_prefix('+') {
            Some(theme) => self.fonts.get(theme)?.as_str(),
            None => name,
        };
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        Some(if name.contains(' ') { format!("\"{name}\", sans-serif") } else { format!("{name}, sans-serif") })
    }
}

struct Package<'z, R> {
    zip: &'z mut zip::ZipArchive<R>,
}

impl<R: Read + Seek> Package<'_, R> {
    fn bytes(&mut self, path: &str) -> Result<Vec<u8>, ImportError> {
        let path = path.trim_start_matches('/');
        let mut entry = self.zip.by_name(path).map_err(ImportError::Zip)?;
        let mut out = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut out)?;
        Ok(out)
    }

    fn xml(&mut self, path: &str) -> Result<Node, ImportError> {
        let bytes = self.bytes(path)?;
        dom::parse(&bytes).map_err(|message| ImportError::Xml { entry: path.to_owned(), message })
    }

    /// Relationship id → target path for a part. Missing rels are empty.
    fn rels(&mut self, part: &str) -> HashMap<String, String> {
        let part = part.trim_start_matches('/');
        let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
        let path = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let Ok(node) = self.xml(&path) else { return HashMap::new() };
        node.children("Relationship")
            .map(|r| (r.a("Id").to_owned(), r.a("Target").trim_start_matches('/').to_owned()))
            .collect()
    }

    fn rel_targets_of_type(&mut self, part: &str, kind: &str) -> Vec<String> {
        let part = part.trim_start_matches('/');
        let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
        let path = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let Ok(node) = self.xml(&path) else { return Vec::new() };
        node.children("Relationship")
            .filter(|r| r.a("Type") == kind || r.a("Type").ends_with(&format!("/{kind}")))
            .map(|r| r.a("Target").trim_start_matches('/').to_owned())
            .collect()
    }

    fn names(&self) -> Vec<String> {
        self.zip.file_names().filter_map(|n| n.ok().map(|n| n.into_owned())).collect()
    }
}

struct Importer {
    warnings: BTreeSet<String>,
    var_kinds: HashMap<Id, ValueKind>,
    /// Layout id → colour palette of the master that owns it.
    palettes: HashMap<String, Palette>,
    default_palette: Palette,
    /// Storyline media id → (display name, bytes).
    media: HashMap<String, (String, Vec<u8>)>,
    /// Storyline media id → imported asset id.
    assets: HashMap<String, Id>,
    doc: Document,
}

pub(crate) fn import<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, title: &str) -> Result<Imported, ImportError> {
    let mut pkg = Package { zip };
    let main = pkg.rel_targets_of_type("", "main").into_iter().next().unwrap_or_else(|| "story/story.xml".to_owned());
    let story = pkg.xml(&main).map_err(|e| match e {
        ImportError::Zip(_) => ImportError::Unsupported("the archive has no story/story.xml part".into()),
        e => e,
    })?;
    if story.name != "story" {
        return Err(ImportError::Unsupported(format!("main part is <{}>, expected <story>", story.name)));
    }

    let mut project = Project::new(title);
    project.scenes.clear();
    let mut im = Importer {
        warnings: BTreeSet::new(),
        var_kinds: HashMap::new(),
        palettes: HashMap::new(),
        default_palette: Palette::default(),
        media: HashMap::new(),
        assets: HashMap::new(),
        doc: Document::new(project),
    };

    im.read_project_props(&story);
    im.read_variables(&story);
    im.read_palettes(&mut pkg);
    im.read_media(&mut pkg, &story)?;

    let story_rels = pkg.rels(&main);
    let mut scenes = Vec::new();
    for (si, scene) in story.path(&["sceneLst"]).into_iter().flat_map(|l| l.children("scene")).enumerate() {
        if !matches!(scene.a("sceneType"), "" | "scene") {
            continue;
        }
        let mut slides = Vec::new();
        for sld_id in scene.path(&["sldIdLst"]).into_iter().flat_map(|l| l.children("sldId")) {
            let Some(target) = story_rels.get(sld_id.text.trim()) else {
                im.warn(format!("slide reference {} has no target", sld_id.text.trim()));
                continue;
            };
            let node = pkg.xml(target)?;
            slides.push(im.slide(&node, slides.len() + 1));
        }
        if slides.is_empty() {
            continue;
        }
        let name = non_empty(scene.a("name")).unwrap_or_else(|| format!("Scene {}", si + 1));
        scenes.push(Scene { id: guid(scene).unwrap_or_default(), name, slides });
    }
    if scenes.is_empty() {
        return Err(ImportError::Unsupported("the project contains no slides".into()));
    }
    im.doc.project.scenes = scenes;

    for problem in im.doc.project.validate() {
        im.warn(problem);
    }
    Ok(Imported { document: im.doc, warnings: im.warnings.into_iter().collect() })
}

impl Importer {
    fn warn(&mut self, message: impl Into<String>) {
        self.warnings.insert(message.into());
    }

    fn read_project_props(&mut self, story: &Node) {
        for prop in story.path(&["propLst"]).into_iter().flat_map(|l| l.children("prop")) {
            match prop.a("id") {
                "15" => {
                    if let Some(sz) = prop.child("sz") {
                        if let (Some(w), Some(h)) = (sz.attr_f32("w"), sz.attr_f32("h")) {
                            if w > 0.0 && h > 0.0 {
                                self.doc.project.slide_size = Size { width: w, height: h };
                            }
                        }
                    }
                }
                "18" => self.doc.project.author = prop.text.trim().to_owned(),
                _ => {}
            }
        }
    }

    fn read_variables(&mut self, story: &Node) {
        for var in story.path(&["varLst"]).into_iter().flat_map(|l| l.children("var")) {
            if var.a("type") != "user" {
                continue;
            }
            let value = match var.a("dataType") {
                "num" => Value::Number(var.a("val").parse().unwrap_or(0.0)),
                "bool" => Value::Bool(var.a("val").eq_ignore_ascii_case("true")),
                "text" => Value::Text(var.a("val").to_owned()),
                other => {
                    self.warn(format!("variable {:?} has unknown type {other:?}; imported as text", var.a("name")));
                    Value::Text(var.a("val").to_owned())
                }
            };
            let id = Id::from(var.a("g"));
            self.var_kinds.insert(id.clone(), value.kind());
            self.doc.project.variables.push(Variable { id, name: var.a("name").to_owned(), value });
        }
    }

    fn read_palettes<R: Read + Seek>(&mut self, pkg: &mut Package<'_, R>) {
        let mut masters: Vec<String> = pkg
            .names()
            .into_iter()
            .filter(|n| n.starts_with("story/slideMasters/") && n.ends_with(".xml") && !n.contains("/_rels/"))
            .collect();
        masters.sort();
        for master_path in masters {
            let Ok(master) = pkg.xml(&master_path) else { continue };
            let colors: HashMap<String, Color> = master
                .child("clrLst")
                .into_iter()
                .flat_map(|list| list.children.iter())
                .filter_map(|entry| Some((entry.name.clone(), srgb(entry.child("srgbClr")?.a("val"))?)))
                .collect();
            let fonts = master
                .child("fonts")
                .into_iter()
                .flat_map(|f| ["major", "minor"].map(|k| (k.to_owned(), f.a(k).to_owned())))
                .filter(|(_, v)| !v.is_empty())
                .collect();
            let palette = Palette { colors, fonts };
            if self.default_palette.colors.is_empty() {
                self.default_palette = palette.clone();
            }
            self.palettes.insert(master.a("g").to_owned(), palette.clone());
            for layout_path in pkg.rel_targets_of_type(&master_path, "slideLayout") {
                if let Ok(layout) = pkg.xml(&layout_path) {
                    self.palettes.insert(layout.a("g").to_owned(), palette.clone());
                }
            }
        }
    }

    fn read_media<R: Read + Seek>(&mut self, pkg: &mut Package<'_, R>, story: &Node) -> Result<(), ImportError> {
        let mut by_hash: HashMap<String, String> = HashMap::new();
        for name in pkg.names().into_iter().filter(|n| n.starts_with("story/media/")) {
            let bytes = pkg.bytes(&name)?;
            by_hash.insert(hex(&Md5::digest(&bytes)), name);
        }
        let list = story.path(&["mediaLst", "mediaLst"]).or_else(|| story.path(&["mediaLst"]));
        for media in list.into_iter().flat_map(|l| l.children("media")) {
            let hash = media.path(&["md5Checksum", "stream"]).map(|n| n.text.trim().to_ascii_lowercase());
            let Some(path) = hash.and_then(|h| by_hash.get(&h)) else {
                self.warn(format!("media {:?} is missing from the package", media.a("displayName")));
                continue;
            };
            let bytes = pkg.bytes(path)?;
            let name = non_empty(media.a("displayName")).unwrap_or_else(|| path.rsplit('/').next().unwrap().to_owned());
            self.media.insert(media.a("g").to_owned(), (name, bytes));
        }
        Ok(())
    }

    fn asset_for(&mut self, media_id: &str) -> Option<Id> {
        if let Some(id) = self.assets.get(media_id) {
            return Some(id.clone());
        }
        let (name, bytes) = self.media.get(media_id)?.clone();
        let id = self.doc.add_asset(&name, mime_for(&name), bytes);
        self.assets.insert(media_id.to_owned(), id.clone());
        Some(id)
    }

    // --- slides ----------------------------------------------------------

    fn slide(&mut self, sld: &Node, number: usize) -> Slide {
        let palette = self.palettes.get(sld.a("layoutG")).cloned().unwrap_or_else(|| self.default_palette.clone());
        let mut slide = Slide::new(non_empty(sld.a("name")).unwrap_or_else(|| format!("Slide {number}")));
        if let Some(id) = guid(sld) {
            slide.id = id;
        }
        slide.advance = match sld.a("advMode") {
            "user" | "" => Advance::ByUser,
            "auto" => Advance::Automatically,
            other => {
                self.warn(format!("slide {:?}: unknown advance mode {other:?}; using By user", slide.name));
                Advance::ByUser
            }
        };
        if let Some(c) = sld.path(&["bg", "solidFill", "clr"]).and_then(|c| color(c, &palette, None)) {
            slide.background = c;
        }
        slide.base = self.layer(sld, "Base Layer", &palette, true);
        slide.base.id = Id::from(format!("{}-base", slide.id));
        for layer in sld.path(&["sldLayerLst"]).into_iter().flat_map(|l| l.children("sldLayer")) {
            let name = non_empty(layer.a("name")).unwrap_or_else(|| "Layer".to_owned());
            let mut l = self.layer(layer, &name, &palette, false);
            l.pause_base_timeline = layer.a("pause") == "true";
            l.hide_other_layers = layer.a("stop") != "false";
            l.prevent_base_clicks = layer.a("modal") == "true";
            l.hide_when_timeline_ends = layer.a("hideEnd") == "true";
            slide.layers.push(l);
        }
        slide
    }

    fn layer(&mut self, node: &Node, name: &str, palette: &Palette, is_base: bool) -> Layer {
        let mut layer = Layer::new(name);
        if let Some(id) = guid(node) {
            layer.id = id;
        }
        if let Some(ms) = node.child("tmProps").and_then(|t| t.attr_f32("min")) {
            layer.duration_ms = ms.max(0.0) as u32;
        }
        for shape in node.path(&["shapeLst"]).into_iter().flat_map(|l| l.children.iter()) {
            if let Some((object, triggers)) = self.object(shape, palette) {
                layer.objects.push(object);
                layer.triggers.extend(triggers);
            }
        }
        for trig in node.path(&["trigLst"]).into_iter().flat_map(|l| l.children("trig")) {
            if let Some(t) = self.trigger(trig, None, is_base) {
                layer.triggers.push(t);
            }
        }
        layer
    }

    // --- objects -----------------------------------------------------------

    fn object(&mut self, node: &Node, palette: &Palette) -> Option<(Object, Vec<Trigger>)> {
        let loc = node.child("loc")?;
        let (l, t, r, b) = (loc.attr_f32("l")?, loc.attr_f32("t")?, loc.attr_f32("r")?, loc.attr_f32("b")?);
        let frame = Rect::new(l, t, (r - l).max(1.0), (b - t).max(1.0));
        let name = non_empty(node.a("typeName")).or_else(|| non_empty(node.a("name"))).unwrap_or_default();

        let kind = match node.name.as_str() {
            "textBox" => ObjectKind::Text,
            "rect" | "roundRect" => ObjectKind::Shape { shape: ShapeKind::Rectangle },
            "oval" => ObjectKind::Shape { shape: ShapeKind::Ellipse },
            "triangle" => ObjectKind::Shape { shape: ShapeKind::Triangle },
            "btn" => ObjectKind::Button,
            "pic" => match self.asset_for(node.a("assetG")) {
                Some(asset) => ObjectKind::Image { asset },
                None => {
                    self.warn(format!("picture {name:?} has no image data; skipped"));
                    return None;
                }
            },
            other => {
                self.warn(format!("<{other}> objects are not supported yet; imported as rectangles"));
                ObjectKind::Shape { shape: ShapeKind::Rectangle }
            }
        };

        let mut object = Object::new(name, kind, frame);
        if let Some(id) = guid(node) {
            object.id = id;
        }
        if let Some(rot) = node.attr_f32("rot").filter(|r| *r > 0.0) {
            object.rotation = rot;
        }
        if node.name == "roundRect" || node.name == "btn" {
            object.style.corner_radius = (frame.width.min(frame.height) * 0.15).round();
        }
        // Buttons and other styled objects keep their look in the Normal state.
        let normal_look = node
            .path(&["stateLst"])
            .and_then(|l| l.children("state").find(|s| s.a("name") == state_names::NORMAL))
            .and_then(|s| s.path(&["shapeLst"]))
            .and_then(|l| l.children.first());
        object.style.fill = self.fill(node, palette, None).or_else(|| self.fill(normal_look?, palette, None));
        object.style.stroke = stroke(node, palette).or_else(|| stroke(normal_look?, palette));
        object.text = match (self.text(node, palette, &object.kind), normal_look) {
            (Some(mut t), Some(look)) => {
                if let Some(c) = look.path(&["bG", "foreClr"]).and_then(|c| color(c, palette, None)) {
                    t.color = c;
                }
                Some(t)
            }
            (Some(t), None) => Some(t),
            (None, look) => look.and_then(|look| self.text(look, palette, &object.kind)),
        };
        object.timing = timing(node);
        match node.a("initState") {
            "" | "Normal" => {}
            "Hidden" => object.initially_hidden = true,
            other => self.warn(format!("object {:?} starts in state {other:?}; it will start in Normal", object.name)),
        }
        object.states = self.states(node, &object, palette);
        if let Some(alt) = node.child("altTextGenerationData").and_then(|a| non_empty(a.a("originalText"))) {
            object.alt_text = alt;
        }

        let triggers = node
            .path(&["trigLst"])
            .into_iter()
            .flat_map(|l| l.children("trig"))
            .filter_map(|t| self.trigger(t, Some(&object.id), false))
            .collect();
        Some((object, triggers))
    }

    /// The object's fill. `base` resolves colours that are stored only as
    /// modifiers of another colour (state looks such as "Hover: 85% tint").
    fn fill(&mut self, node: &Node, palette: &Palette, base: Option<Color>) -> Option<Color> {
        let bg = node.child("bG")?;
        if bg.child("noFill").is_some() {
            return None;
        }
        color(bg.path(&["solidFill", "clr"])?, palette, base)
    }

    fn text(&mut self, node: &Node, palette: &Palette, kind: &ObjectKind) -> Option<TextContent> {
        let raw = node.child("text").map(|t| t.text.as_str()).unwrap_or("");
        let (mut text, style) = if raw.trim().is_empty() { Default::default() } else { self.rich_text(raw) };
        if text.is_empty() {
            text = node.child("plain").map(|p| p.text.clone()).unwrap_or_default();
        }
        if text.is_empty() {
            return None;
        }
        let mut content = TextContent::new(text);
        // Unstyled text uses the object's default font, usually the theme's.
        let default_font = node.child("resize").and_then(|r| palette.font(r.a("fontNm")));
        if let Some(family) = default_font.or_else(|| palette.font("+minor")) {
            content.font_family = family;
        }
        content.font_size =
            node.child("resize").and_then(|r| r.attr_f32("fontSz")).filter(|s| *s > 0.0).unwrap_or(DEFAULT_FONT_SIZE);
        if let Some(c) = node.path(&["bG", "foreClr"]).and_then(|c| color(c, palette, None)) {
            content.color = c;
        } else if matches!(kind, ObjectKind::Button) {
            content.color = Color::WHITE;
        }
        if matches!(kind, ObjectKind::Text) {
            content.align = HAlign::Left;
            content.valign = VAlign::Top;
        }
        let family = style.family.as_deref().and_then(|f| palette.font(f));
        style.apply(&mut content);
        if let Some(family) = family {
            content.font_family = family;
        }
        Some(content)
    }

    /// Flatten Storyline's embedded text document into plain text, one line
    /// per block, plus the formatting of its first styled block and span.
    /// StoryCraft has one style per text box, so mixed formatting is
    /// simplified (and noted).
    fn rich_text(&mut self, raw: &str) -> (String, TextStyle) {
        let Ok(doc) = dom::parse(raw.as_bytes()) else {
            self.warn("some text could not be read; using its plain-text copy");
            return Default::default();
        };
        let blocks: Vec<&Node> = doc.path(&["Content"]).into_iter().flat_map(|c| c.children("Block")).collect();
        let lines: Vec<String> = blocks.iter().map(|b| b.children("Span").map(|s| s.a("Text")).collect()).collect();

        let mut style = TextStyle::default();
        let block_styles: Vec<&Node> = blocks.iter().filter_map(|b| b.child("Style")).collect();
        let spans: Vec<&Node> = blocks.iter().flat_map(|b| b.children("Span")).collect();
        let span_styles: Vec<&Node> = spans.iter().filter_map(|s| s.child("Style")).collect();
        if let Some(first) = block_styles.first() {
            style.align = match first.a("Justification") {
                "Left" | "Justify" => Some(HAlign::Left),
                "Center" => Some(HAlign::Center),
                "Right" => Some(HAlign::Right),
                _ => None,
            };
        }
        if let Some(first) = span_styles.first() {
            let flag = |name: &str| first.attr(name).map(|v| v.eq_ignore_ascii_case("true"));
            style.bold = flag("FontIsBold");
            style.italic = flag("FontIsItalic");
            style.color = first.attr("ForegroundColor").and_then(|c| c.parse().ok());
            style.size = first.attr_f32("FontSize").filter(|s| *s > 0.0);
            style.family = first.attr("FontFamily").and_then(non_empty);
        }
        let distinct = |nodes: &[&Node]| nodes.iter().map(|n| n.attrs.clone()).collect::<BTreeSet<_>>().len();
        let partly_styled = !span_styles.is_empty() && span_styles.len() < spans.len();
        if distinct(&span_styles) > 1 || distinct(&block_styles) > 1 || partly_styled {
            self.warn("text with mixed formatting was simplified to one style per text box");
        }
        (lines.join("\n"), style)
    }

    fn states(&mut self, node: &Node, object: &Object, palette: &Palette) -> Vec<ObjectState> {
        let base_text = object.text.as_ref().map(|t| t.text.clone());
        let mut states = Vec::new();
        for state in node.path(&["stateLst"]).into_iter().flat_map(|l| l.children("state")) {
            let name = state.a("name");
            if name.is_empty() || name == state_names::NORMAL || name == state_names::HIDDEN {
                continue;
            }
            let mut overrides = StateOverrides::default();
            if let Some(look) = state.path(&["shapeLst"]).and_then(|l| l.children.first()) {
                let fill = self.fill(look, palette, object.style.fill);
                if fill.is_some() && fill != object.style.fill {
                    overrides.fill = fill;
                }
                let line = stroke(look, palette).map(|s| s.color);
                if line.is_some() && line != object.style.stroke.map(|s| s.color) {
                    overrides.stroke_color = line;
                }
                let text = self.text(look, palette, &object.kind);
                if let Some(t) = &text {
                    if Some(&t.text) != base_text.as_ref() {
                        overrides.text = Some(t.text.clone());
                    }
                    if object.text.as_ref().is_some_and(|base| base.color != t.color) {
                        overrides.text_color = Some(t.color);
                    }
                }
            }
            states.push(ObjectState { name: name.to_owned(), overrides });
        }
        states
    }

    // --- triggers ----------------------------------------------------------

    fn trigger(&mut self, trig: &Node, owner: Option<&Id>, on_base: bool) -> Option<Trigger> {
        let data = trig.child("data")?;
        let event_name = data.a("event");
        // The player's own Next/Previous buttons are built into StoryCraft's
        // player, so the slide triggers that drive them are not imported.
        if owner.is_none() && matches!(data.a("NavigationIntent"), "Next" | "Previous") {
            return None;
        }
        let _ = on_base;

        let event = match (event_name, owner) {
            ("OnClick" | "OnRelease", Some(o)) => Event::Click { object: o.clone() },
            ("OnRightClick", Some(o)) => {
                self.warn("\"user right-clicks\" triggers were imported as ordinary clicks");
                Event::Click { object: o.clone() }
            }
            ("OnRollOver", Some(o)) => Event::HoverStart { object: o.clone() },
            ("OnRollOut", Some(o)) => Event::HoverEnd { object: o.clone() },
            ("OnStart", _) => Event::TimelineStarts,
            ("OnEnd" | "OnComplete" | "OnTimelineEnd", _) => Event::TimelineEnds,
            ("OnVariableValueChange", _) => {
                match data.child("other").and_then(|o| o.attr("varChangeG")).and_then(non_nil) {
                    Some(variable) => Event::VariableChanges { variable },
                    None => {
                        self.warn("a \"variable changes\" trigger has no variable; it was skipped");
                        return None;
                    }
                }
            }
            ("OnKeyPress", _) => {
                Event::KeyPressed { key: data.child("keyPress").map(|k| k.a("keys").to_owned()).unwrap_or_default() }
            }
            (other, _) => {
                self.warn(format!("trigger event {other:?} is not supported yet; those triggers were skipped"));
                return None;
            }
        };

        let action = self.action(data)?;
        let mut trigger = Trigger::new(action, event);
        if let Some(id) = guid(trig) {
            trigger.id = id;
        }
        trigger.enabled = data.a("enabled") != "false";
        trigger.conditions = trig
            .path(&["condLst"])
            .into_iter()
            .flat_map(|l| l.children("trigCond"))
            .filter_map(|c| self.condition(c))
            .collect();
        Some(trigger)
    }

    fn action(&mut self, data: &Node) -> Option<Action> {
        let sub = data.a("actSubType");
        let action = match data.a("action") {
            "jumpToSlide" => Action::JumpToSlide {
                target: match sub {
                    "next" => SlideTarget::Next,
                    "prev" => SlideTarget::Previous,
                    _ => match data.child("slide").and_then(|s| s.attr("jumpG")).and_then(non_nil) {
                        Some(id) => SlideTarget::Slide(id),
                        None => {
                            self.warn("a \"jump to slide\" target could not be read; it jumps to the next slide");
                            SlideTarget::Next
                        }
                    },
                },
            },
            "showSubSlide" => match data.child("sldLayer").and_then(|l| l.attr("showG")).and_then(non_nil) {
                Some(layer) => Action::ShowLayer { layer },
                None => {
                    self.warn("a \"show layer\" target could not be read; that trigger was skipped");
                    return None;
                }
            },
            "hideSubSlide" => Action::HideLayer {
                layer: match (sub, data.child("sldLayer").and_then(|l| l.attr("hideG")).and_then(non_nil)) {
                    ("me", _) | (_, None) => LayerTarget::This,
                    (_, Some(id)) => LayerTarget::Layer(id),
                },
            },
            "changeShapeState" => {
                let shape = data.child("shape")?;
                let Some(object) = shape.attr("setStateG").and_then(non_nil) else {
                    self.warn("a \"change state\" target could not be read; that trigger was skipped");
                    return None;
                };
                Action::ChangeState { object, state: shape.a("setStateName").to_owned() }
            }
            "adjustVar" => {
                let other = data.child("other")?;
                let variable = non_nil(other.a("varG"))?;
                let kind = self.var_kinds.get(&variable).copied().unwrap_or(ValueKind::Number);
                let op = match other.a("op") {
                    "ass" => VarOp::Assign,
                    "add" => VarOp::Add,
                    "sub" => VarOp::Subtract,
                    "mul" => VarOp::Multiply,
                    "div" => VarOp::Divide,
                    "not" | "notass" => VarOp::Toggle,
                    o => {
                        self.warn(format!("variable operator {o:?} is not supported; used assignment"));
                        VarOp::Assign
                    }
                };
                let value = if other.a("useVar") == "true" {
                    match other.attr("varG2").and_then(non_nil) {
                        Some(v) => Operand::Variable(v),
                        None => {
                            self.warn("a variable-to-variable assignment could not be read; used a literal value");
                            Operand::Literal(literal(other.a("dblVal"), other.a("boolVal"), other.a("varVal1"), kind))
                        }
                    }
                } else {
                    Operand::Literal(literal(other.a("dblVal"), other.a("boolVal"), other.a("varVal1"), kind))
                };
                Action::SetVariable { variable, op, value }
            }
            other => {
                self.warn(format!("trigger action {other:?} is not supported yet; those triggers were skipped"));
                return None;
            }
        };
        Some(action)
    }

    fn condition(&mut self, c: &Node) -> Option<ConditionClause> {
        let join = if c.a("andOr").eq_ignore_ascii_case("or") { Join::Or } else { Join::And };
        let condition = match c.a("dataType") {
            "state" => {
                let state = c.child("shapeState")?;
                Condition::ObjectState {
                    object: non_nil(state.a("shapeG"))?,
                    state: state.a("name").to_owned(),
                    negate: state.a("op") == "noteq",
                }
            }
            "value" | "var" | "variable" => {
                let variable = non_nil(c.a("varG"))?;
                let kind = self.var_kinds.get(&variable).copied().unwrap_or(ValueKind::Number);
                let value = match non_nil(c.a("varG2")) {
                    Some(other) => Operand::Variable(other),
                    None => Operand::Literal(literal(c.a("floatVal1"), c.a("boolVal"), c.a("strVal1"), kind)),
                };
                let cmp = match c.a("op") {
                    "eq" => Comparison::Eq,
                    "noteq" | "neq" => Comparison::Ne,
                    "gt" => Comparison::Gt,
                    "lt" => Comparison::Lt,
                    "gteq" | "gte" | "ge" => Comparison::Ge,
                    "lteq" | "lte" | "le" => Comparison::Le,
                    "contains" => Comparison::Contains,
                    "notcontains" => Comparison::NotContains,
                    o => {
                        self.warn(format!("condition operator {o:?} is not supported; used =="));
                        Comparison::Eq
                    }
                };
                Condition::Variable { variable, cmp, value }
            }
            other => {
                self.warn(format!("condition type {other:?} is not supported; those conditions were skipped"));
                return None;
            }
        };
        Some(ConditionClause { join, condition })
    }
}

/// The object's outline: `bG/solidLine/clr`, with its width in `lineStyle@w`.
fn stroke(node: &Node, palette: &Palette) -> Option<Stroke> {
    let bg = node.child("bG")?;
    if bg.child("noLine").is_some() {
        return None;
    }
    let color = color(bg.path(&["solidLine", "clr"])?, palette, None)?;
    let width = bg.child("lineStyle").and_then(|l| l.attr_f32("w")).unwrap_or(1.0);
    (width > 0.0).then_some(Stroke { color, width })
}

fn timing(node: &Node) -> Timing {
    let Some(ctx) = node.path(&["tmCtxLst"]).and_then(|l| l.children.first()) else { return Timing::default() };
    let start = ctx.attr_f32("start").unwrap_or(0.0).max(0.0) as u32;
    let duration = (ctx.a("untilEnd") != "true").then(|| ctx.attr_f32("dur").unwrap_or(0.0).max(0.0) as u32);
    Timing { start_ms: start, duration_ms: duration.filter(|d| *d > 0) }
}

/// Formatting read from a text document; `None` keeps the default.
#[derive(Default)]
struct TextStyle {
    align: Option<HAlign>,
    bold: Option<bool>,
    italic: Option<bool>,
    color: Option<Color>,
    size: Option<f32>,
    family: Option<String>,
}

impl TextStyle {
    fn apply(self, t: &mut TextContent) {
        if let Some(a) = self.align {
            t.align = a;
        }
        if let Some(b) = self.bold {
            t.bold = b;
        }
        if let Some(i) = self.italic {
            t.italic = i;
        }
        if let Some(c) = self.color {
            t.color = c;
        }
        if let Some(size) = self.size {
            t.font_size = size;
        }
    }
}

fn literal(number: &str, boolean: &str, text: &str, kind: ValueKind) -> Value {
    match kind {
        ValueKind::Number => Value::Number(number.parse().unwrap_or(0.0)),
        // An empty boolean value is how False is written.
        ValueKind::Bool => Value::Bool(boolean.eq_ignore_ascii_case("true")),
        ValueKind::Text => Value::Text(text.to_owned()),
    }
}

/// Resolve a `clr` element: an srgbClr or schemeClr plus modifiers, or
/// modifiers alone, which apply to `relative_to`.
fn color(clr: &Node, palette: &Palette, relative_to: Option<Color>) -> Option<Color> {
    let (base, mods) = if let Some(s) = clr.child("srgbClr") {
        (srgb(s.a("val"))?, s)
    } else if let Some(s) = clr.child("schemeClr") {
        let key = match s.a("val") {
            "tx1" => "dk1",
            "bg1" => "lt1",
            "tx2" => "dk2",
            "bg2" => "lt2",
            k => k,
        };
        (*palette.get(key)?, s)
    } else if clr.children.is_empty() {
        return None;
    } else {
        (relative_to?, clr)
    };
    // Modifiers may sit inside the colour element or beside it.
    let beside: &[Node] = if std::ptr::eq(mods, clr) { &[] } else { &clr.children };
    let modifiers = mods.children.iter().chain(beside.iter().filter(|c| !c.name.ends_with("Clr")));
    let mut c = base;
    for m in modifiers {
        let v = m.attr_f32("val").unwrap_or(100_000.0) / 100_000.0;
        let ch = |x: u8, f: &dyn Fn(f32) -> f32| f(x as f32).round().clamp(0.0, 255.0) as u8;
        match m.name.as_str() {
            "tint" => {
                let f = |x: f32| x + (255.0 - x) * (1.0 - v);
                c = Color::rgba(ch(c.r, &f), ch(c.g, &f), ch(c.b, &f), c.a);
            }
            "shade" | "lumMod" => {
                let f = |x: f32| x * v;
                c = Color::rgba(ch(c.r, &f), ch(c.g, &f), ch(c.b, &f), c.a);
            }
            "lumOff" => {
                let f = |x: f32| x + 255.0 * v;
                c = Color::rgba(ch(c.r, &f), ch(c.g, &f), ch(c.b, &f), c.a);
            }
            "alpha" => c.a = (v * 255.0).round().clamp(0.0, 255.0) as u8,
            _ => {}
        }
    }
    Some(c)
}

fn srgb(hex: &str) -> Option<Color> {
    format!("#{hex}").parse().ok()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn guid(node: &Node) -> Option<Id> {
    node.attr("g").and_then(non_nil)
}

fn non_nil(s: &str) -> Option<Id> {
    (!s.is_empty() && s != NIL_GUID).then(|| Id::from(s))
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}
