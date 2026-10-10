//! Import of Articulate Storyline `.story` projects.
//!
//! # Clean-room policy
//!
//! This crate is written only from what can be observed in `.story` files
//! that their authors created and are entitled to share, plus public
//! documentation. It must never be based on Storyline's program code
//! (decompiled, disassembled or otherwise), and contributors must not paste
//! Articulate code or copyrighted sample content into this repository.
//! See `docs/clean-room.md`.
//!
//! # Status
//!
//! The format mapping is built up from sample files. Until a construct has
//! been mapped, [`import`] reports [`ImportError::NotYetMapped`]. Use
//! [`inspect`] (`storycraft-cli inspect file.story`) to produce the structural
//! report the mapping is written from.

mod inspect;

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

use storycraft_model::archive::Document;

pub use inspect::{ElementStats, EntryInfo, EntryKind, Inspection};

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a Storyline project (expected a ZIP-based .story file): {0}")]
    NotAStoryFile(zip::result::ZipError),
    #[error("could not read the project archive: {0}")]
    Zip(zip::result::ZipError),
    #[error("malformed XML in {entry}: {message}")]
    Xml { entry: String, message: String },
    #[error(
        "importing .story projects is not mapped yet. Run `storycraft-cli inspect` on sample files \
         and see docs/story-import.md to help map the format"
    )]
    NotYetMapped,
}

/// The result of an import: the converted project plus notes about anything
/// that could not be carried over exactly.
pub struct Imported {
    pub document: Document,
    pub warnings: Vec<String>,
}

fn open_zip<R: Read + Seek>(reader: R) -> Result<zip::ZipArchive<R>, ImportError> {
    zip::ZipArchive::new(reader).map_err(ImportError::NotAStoryFile)
}

/// Describe the structure of a `.story` file: its entries and, for XML
/// entries, every element path with its attributes. Contains no slide text
/// beyond short attribute samples.
pub fn inspect(path: impl AsRef<Path>) -> Result<Inspection, ImportError> {
    let file = BufReader::new(File::open(path)?);
    inspect::inspect(&mut open_zip(file)?)
}

/// Convert a `.story` project into a StoryCraft document.
pub fn import(path: impl AsRef<Path>) -> Result<Imported, ImportError> {
    let file = BufReader::new(File::open(path)?);
    let _zip = open_zip(file)?;
    Err(ImportError::NotYetMapped)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;

    fn fake_story() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("project/data.xml", opts).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><root a="1"><child b="x"/><child b="y"><leaf/></child></root>"#)
            .unwrap();
        zip.start_file("media/pic.png", opts).unwrap();
        zip.write_all(b"\x89PNG\r\n").unwrap();
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn inspects_xml_structure() {
        let mut zip = open_zip(Cursor::new(fake_story())).unwrap();
        let report = inspect::inspect(&mut zip).unwrap();
        assert_eq!(report.entries.len(), 2);
        let EntryKind::Xml { root, elements } = &report.entries[0].kind else { panic!("expected xml") };
        assert_eq!(root, "root");
        assert_eq!(elements["root/child"].count, 2);
        let samples = &elements["root/child"].attributes["b"];
        assert!(samples.contains("x") && samples.contains("y"));
        assert_eq!(elements["root/child/leaf"].count, 1);
        assert!(matches!(&report.entries[1].kind, EntryKind::Binary { magic } if magic == "PNG image"));
        assert!(report.to_string().contains("<child> ×2"), "{report}");
    }

    #[test]
    fn rejects_non_zip() {
        assert!(matches!(open_zip(Cursor::new(b"not a zip".to_vec())), Err(ImportError::NotAStoryFile(_))));
    }
}
