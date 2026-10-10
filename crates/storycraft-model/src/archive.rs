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
        _ => "application/octet-stream",
    }
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
    fn rejects_newer_format() {
        let mut doc = Document::new(Project::new("x"));
        doc.project.format_version = FORMAT_VERSION + 1;
        let mut buf = std::io::Cursor::new(Vec::new());
        doc.write(&mut buf).unwrap();
        buf.set_position(0);
        assert!(matches!(Document::read(buf), Err(ArchiveError::TooNew { .. })));
    }
}
