//! The `.storycraft` file format: a ZIP archive holding `project.json` and a
//! `media/` folder with every asset's bytes.

use std::collections::BTreeMap;
use std::io::{Read, Seek, Write};

use zip::write::SimpleFileOptions;

use crate::{FORMAT_VERSION, Id, Project};

pub const EXTENSION: &str = "storycraft";
const PROJECT_ENTRY: &str = "project.json";
const MEDIA_DIR: &str = "media/";

/// A project together with the bytes of its assets.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub project: Project,
    /// Asset bytes keyed by asset id.
    pub media: BTreeMap<Id, Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a valid archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("project.json is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive has no project.json")]
    MissingProject,
    #[error("project was saved by a newer StoryCraft (format {found}, this build reads up to {FORMAT_VERSION})")]
    TooNew { found: u32 },
    #[error("asset {name:?} is missing its file media/{file}")]
    MissingMedia { name: String, file: String },
}

impl Document {
    pub fn new(project: Project) -> Self {
        Document { project, media: BTreeMap::new() }
    }

    pub fn read(reader: impl Read + Seek) -> Result<Self, ArchiveError> {
        let mut zip = zip::ZipArchive::new(reader)?;
        let project: Project = {
            let entry = zip.by_name(PROJECT_ENTRY).map_err(|e| match e {
                zip::result::ZipError::FileNotFound => ArchiveError::MissingProject,
                e => e.into(),
            })?;
            serde_json::from_reader(entry)?
        };
        if project.format_version > FORMAT_VERSION {
            return Err(ArchiveError::TooNew { found: project.format_version });
        }

        let mut media = BTreeMap::new();
        for asset in &project.assets {
            let path = format!("{MEDIA_DIR}{}", asset.file_name);
            let mut entry = zip
                .by_name(&path)
                .map_err(|_| ArchiveError::MissingMedia { name: asset.name.clone(), file: asset.file_name.clone() })?;
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes)?;
            media.insert(asset.id.clone(), bytes);
        }
        Ok(Document { project, media })
    }

    pub fn write(&self, writer: impl Write + Seek) -> Result<(), ArchiveError> {
        let mut zip = zip::ZipWriter::new(writer);
        let deflate = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        // Media is usually already compressed; storing avoids wasted effort.
        let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        zip.start_file(PROJECT_ENTRY, deflate)?;
        serde_json::to_writer_pretty(&mut zip, &self.project)?;

        for asset in &self.project.assets {
            if let Some(bytes) = self.media.get(&asset.id) {
                zip.start_file(format!("{MEDIA_DIR}{}", asset.file_name), stored)?;
                zip.write_all(bytes)?;
            }
        }
        zip.finish()?;
        Ok(())
    }

    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, ArchiveError> {
        let file = std::fs::File::open(path)?;
        Document::read(std::io::BufReader::new(file))
    }

    /// Write to `path` atomically: a crash mid-save leaves the old file intact.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), ArchiveError> {
        let path = path.as_ref();
        let tmp = path.with_extension(format!("{EXTENSION}.tmp"));
        {
            let file = std::fs::File::create(&tmp)?;
            let mut writer = std::io::BufWriter::new(file);
            self.write(&mut writer)?;
            writer.flush()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Add media bytes as a new asset and return its id.
    pub fn add_asset(&mut self, name: &str, mime: &str, bytes: Vec<u8>) -> Id {
        let id = Id::new();
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        let file_name = if ext.is_empty() { id.to_string() } else { format!("{id}.{ext}") };
        self.project.assets.push(crate::Asset {
            id: id.clone(),
            name: name.to_owned(),
            mime: mime.to_owned(),
            file_name,
        });
        self.media.insert(id.clone(), bytes);
        id
    }
}

/// Guess a MIME type from a file name's extension.
pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

/// Pack a folder of web content (recursively) into a ZIP for a web object's
/// bundle. Paths inside the ZIP are relative to `dir` and use `/`.
pub fn bundle_folder(dir: &std::path::Path) -> Result<Vec<u8>, ArchiveError> {
    fn walk(dir: &std::path::Path, prefix: &str, out: &mut Vec<(String, std::path::PathBuf)>) -> std::io::Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("{prefix}{name}");
            if entry.file_type()?.is_dir() {
                walk(&entry.path(), &format!("{path}/"), out)?;
            } else {
                out.push((path, entry.path()));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, "", &mut files)?;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, path) in files {
        zip.start_file(name, options)?;
        zip.write_all(&std::fs::read(path)?)?;
    }
    Ok(zip.finish()?.into_inner())
}

/// The files of a web bundle as (relative path, bytes). Entries whose paths
/// could escape the bundle's folder (`..`, absolute paths) are skipped.
pub fn bundle_files(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, ArchiveError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        let Some(path) = entry.enclosed_name().map(|p| p.to_string_lossy().replace('\\', "/")) else { continue };
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        out.push((path, data));
    }
    Ok(out)
}

/// Choose a bundle's start page: `preferred` if the bundle has it, else
/// `index.html` (at any depth, shallowest first), else the first HTML file.
pub fn bundle_start_page(paths: &[String], preferred: Option<&str>) -> Option<String> {
    let is_html = |p: &str| {
        let p = p.to_ascii_lowercase();
        p.ends_with(".html") || p.ends_with(".htm")
    };
    if let Some(p) = preferred.and_then(|want| paths.iter().find(|p| p.eq_ignore_ascii_case(want))) {
        return Some(p.clone());
    }
    let mut html: Vec<&String> = paths.iter().filter(|p| is_html(p)).collect();
    html.sort_by_key(|p| (p.matches('/').count(), !p.to_ascii_lowercase().ends_with("index.html"), p.to_string()));
    html.first().map(|p| p.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_media() {
        let mut doc = Document::new(crate::sample::sample_project());
        doc.add_asset("pic.PNG", "image/png", vec![1, 2, 3]);

        let mut buf = std::io::Cursor::new(Vec::new());
        doc.write(&mut buf).unwrap();
        buf.set_position(0);
        let back = Document::read(buf).unwrap();
        assert_eq!(back, doc);
        assert!(back.project.assets[0].file_name.ends_with(".png"));
    }

    #[test]
    fn web_bundles_round_trip_and_stay_enclosed() {
        let dir = std::env::temp_dir().join(format!("storycraft-bundle-{}", Id::new()));
        std::fs::create_dir_all(dir.join("css")).unwrap();
        std::fs::write(dir.join("index.html"), "<p>hi</p>").unwrap();
        std::fs::write(dir.join("css/site.css"), "p{}").unwrap();
        let bytes = bundle_folder(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        let files = bundle_files(&bytes).unwrap();
        let paths: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
        assert_eq!(paths, vec!["css/site.css".to_string(), "index.html".to_string()]);
        assert_eq!(bundle_start_page(&paths, None).as_deref(), Some("index.html"));

        // A hostile bundle cannot place files outside its folder.
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("../evil.html", SimpleFileOptions::default()).unwrap();
        zip.write_all(b"x").unwrap();
        zip.start_file("ok/page.htm", SimpleFileOptions::default()).unwrap();
        zip.write_all(b"y").unwrap();
        let files = bundle_files(&zip.finish().unwrap().into_inner()).unwrap();
        let paths: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
        assert_eq!(paths, vec!["ok/page.htm".to_string()]);
        assert_eq!(bundle_start_page(&paths, Some("index.html")).as_deref(), Some("ok/page.htm"));
    }

    #[test]
    fn rejects_newer_format() {
        let mut doc = Document::new(Project::new("x"));
        doc.project.format_version = FORMAT_VERSION + 1;
        let mut buf = std::io::Cursor::new(Vec::new());
        doc.write(&mut buf).unwrap();
        buf.set_position(0);
        assert!(matches!(Document::read(buf), Err(ArchiveError::TooNew { .. })));
    }
}
