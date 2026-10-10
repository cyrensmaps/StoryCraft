//! Publishing: turns a [`Document`] into a self-contained web package.
//!
//! The package runs from any web server, from an LMS, or straight from disk
//! (`file://`): course data and the WebAssembly engine are loaded as plain
//! scripts rather than fetched, which browsers block for local files.
//!
//! ```text
//! index.html
//! imsmanifest.xml          (SCORM only)
//! storycraft/player.js     DOM renderer, input and LMS bridge
//! storycraft/player.css
//! storycraft/engine.js     the WebAssembly engine, base64-encoded
//! storycraft/course.js     the project as JSON
//! media/…                  assets
//! ```

mod base64;
mod scorm12;

use std::io::{Seek, Write};
use std::path::Path;

use storycraft_model::archive::Document;

const PLAYER_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/storycraft_player.wasm"));
const PLAYER_JS: &str = include_str!("../assets/player.js");
const PLAYER_CSS: &str = include_str!("../assets/player.css");
const INDEX_HTML: &str = include_str!("../assets/index.html");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A plain website with no LMS tracking.
    Html,
    /// A SCORM 1.2 package with completion, bookmarking and resume.
    Scorm12,
}

impl Target {
    pub const ALL: [Target; 2] = [Target::Html, Target::Scorm12];

    pub fn label(&self) -> &'static str {
        match self {
            Target::Html => "Web (HTML5)",
            Target::Scorm12 => "LMS (SCORM 1.2)",
        }
    }

    fn id(&self) -> &'static str {
        match self {
            Target::Html => "html",
            Target::Scorm12 => "scorm12",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(
        "this build of StoryCraft has no player runtime; install the wasm32-unknown-unknown target \
         (`rustup target add wasm32-unknown-unknown`) and rebuild"
    )]
    NoPlayerRuntime,
    #[error("the project has no slides")]
    NoSlides,
    #[error("asset {0:?} has no data")]
    MissingMedia(String),
    #[error("could not serialise the course: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not write the package: {0}")]
    Zip(#[from] zip::result::ZipError),
}

/// The files of a published course, in memory.
pub struct Package {
    pub files: Vec<(String, Vec<u8>)>,
    /// Problems that did not stop publishing, such as triggers pointing at
    /// deleted objects. Worth showing to the author.
    pub warnings: Vec<String>,
}

pub fn build(doc: &Document, target: Target) -> Result<Package, ExportError> {
    if PLAYER_WASM.is_empty() {
        return Err(ExportError::NoPlayerRuntime);
    }
    let project = &doc.project;
    if project.slide_count() == 0 {
        return Err(ExportError::NoSlides);
    }
    let warnings = project.validate();

    let mut files = Vec::new();
    let title = html_escape(&project.title);
    let index = INDEX_HTML.replace("{{TITLE}}", &title).replace("{{VERSION}}", env!("CARGO_PKG_VERSION"));
    files.push(("index.html".to_owned(), index.into_bytes()));
    files.push(("storycraft/player.js".to_owned(), PLAYER_JS.as_bytes().to_vec()));
    files.push(("storycraft/player.css".to_owned(), PLAYER_CSS.as_bytes().to_vec()));

    let engine_js = format!("window.STORYCRAFT_WASM = \"{}\";\n", base64::encode(PLAYER_WASM));
    files.push(("storycraft/engine.js".to_owned(), engine_js.into_bytes()));

    let course_js = format!(
        "window.STORYCRAFT_TARGET = \"{}\";\nwindow.STORYCRAFT_COURSE = {};\n",
        target.id(),
        serde_json::to_string(project)?
    );
    files.push(("storycraft/course.js".to_owned(), course_js.into_bytes()));

    for asset in &project.assets {
        let bytes = doc.media.get(&asset.id).ok_or_else(|| ExportError::MissingMedia(asset.name.clone()))?;
        files.push((format!("media/{}", asset.file_name), bytes.clone()));
    }

    if target == Target::Scorm12 {
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        let manifest = scorm12::manifest(project, &paths);
        files.push(("imsmanifest.xml".to_owned(), manifest.into_bytes()));
    }

    Ok(Package { files, warnings })
}

impl Package {
    pub fn write_zip(&self, writer: impl Write + Seek) -> Result<(), ExportError> {
        let mut zip = zip::ZipWriter::new(writer);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (path, bytes) in &self.files {
            zip.start_file(path.as_str(), options)?;
            zip.write_all(bytes)?;
        }
        zip.finish()?;
        Ok(())
    }

    /// Write into `dir`, creating it if needed. Existing files with the same
    /// names are replaced; nothing else in the folder is touched.
    pub fn write_dir(&self, dir: &Path) -> Result<(), ExportError> {
        for (path, bytes) in &self.files {
            let dest = dir.join(path);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(dest, bytes)?;
        }
        Ok(())
    }

    /// Write to `out`: a `.zip` file if the name ends in `.zip`, otherwise a
    /// folder.
    pub fn write_to(&self, out: &Path) -> Result<(), ExportError> {
        let is_zip = out.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
        if is_zip {
            let file = std::fs::File::create(out)?;
            self.write_zip(std::io::BufWriter::new(file))
        } else {
            self.write_dir(out)
        }
    }
}

pub(crate) fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use storycraft_model::sample::sample_project;

    use super::*;

    fn paths(p: &Package) -> Vec<&str> {
        p.files.iter().map(|(p, _)| p.as_str()).collect()
    }

    #[test]
    fn html_package_layout() {
        let mut doc = Document::new(sample_project());
        doc.add_asset("logo.png", "image/png", vec![0x89, b'P', b'N', b'G']);
        let pkg = build(&doc, Target::Html).unwrap();
        let paths = paths(&pkg);
        for expected in ["index.html", "storycraft/player.js", "storycraft/engine.js", "storycraft/course.js"] {
            assert!(paths.contains(&expected), "missing {expected}");
        }
        assert!(paths.iter().any(|p| p.starts_with("media/") && p.ends_with(".png")));
        assert!(!paths.contains(&"imsmanifest.xml"));
        assert!(pkg.warnings.is_empty(), "{:?}", pkg.warnings);
    }

    #[test]
    fn title_is_escaped() {
        let mut doc = Document::new(sample_project());
        doc.project.title = "<script>alert(1)</script>".into();
        let pkg = build(&doc, Target::Html).unwrap();
        let index = String::from_utf8(pkg.files[0].1.clone()).unwrap();
        assert!(!index.contains("<script>alert"));
        assert!(index.contains("&lt;script&gt;"));
    }

    #[test]
    fn scorm_package_has_manifest_listing_files() {
        let doc = Document::new(sample_project());
        let pkg = build(&doc, Target::Scorm12).unwrap();
        let (_, manifest) = pkg.files.iter().find(|(p, _)| p == "imsmanifest.xml").unwrap();
        let manifest = String::from_utf8(manifest.clone()).unwrap();
        assert!(manifest.contains("<schemaversion>1.2</schemaversion>"));
        assert!(manifest.contains(r#"href="storycraft/engine.js""#));
        assert!(manifest.contains(r#"adlcp:scormtype="sco""#));
    }

    #[test]
    fn writes_zip_and_folder() {
        let doc = Document::new(sample_project());
        let pkg = build(&doc, Target::Scorm12).unwrap();
        let dir = tempfile::tempdir().unwrap();
        pkg.write_to(&dir.path().join("course.zip")).unwrap();
        let zip = zip::ZipArchive::new(std::fs::File::open(dir.path().join("course.zip")).unwrap()).unwrap();
        assert_eq!(zip.len(), pkg.files.len());
        pkg.write_to(&dir.path().join("site")).unwrap();
        assert!(dir.path().join("site/storycraft/player.js").exists());
    }

    #[test]
    fn missing_media_is_an_error() {
        let mut doc = Document::new(sample_project());
        let id = doc.add_asset("a.png", "image/png", vec![1]);
        doc.media.remove(&id);
        assert!(matches!(build(&doc, Target::Html), Err(ExportError::MissingMedia(_))));
    }
}
