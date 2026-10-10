//! Structural reports of `.story` archives, used to map the format.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{BufReader, Read, Seek};

use quick_xml::events::Event;

use crate::ImportError;

/// At most this many distinct sample values are kept per attribute.
const MAX_SAMPLES: usize = 6;
/// Sample values are truncated to this many characters.
const MAX_SAMPLE_LEN: usize = 48;

pub struct Inspection {
    pub entries: Vec<EntryInfo>,
}

pub struct EntryInfo {
    pub name: String,
    pub size: u64,
    pub kind: EntryKind,
}

pub enum EntryKind {
    Directory,
    Xml {
        root: String,
        /// Element path (`root/child/leaf`) → statistics.
        elements: BTreeMap<String, ElementStats>,
    },
    Binary {
        magic: String,
    },
}

#[derive(Default)]
pub struct ElementStats {
    pub count: usize,
    /// Attribute name → a few distinct sample values.
    pub attributes: BTreeMap<String, BTreeSet<String>>,
    /// Whether any instance had non-whitespace text content.
    pub has_text: bool,
}

pub(crate) fn inspect<R: Read + Seek>(zip: &mut zip::ZipArchive<R>) -> Result<Inspection, ImportError> {
    let mut entries = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(ImportError::Zip)?;
        let name = entry.name().map_err(ImportError::Zip)?.into_owned();
        let size = entry.size();
        if entry.is_dir() {
            entries.push(EntryInfo { name, size, kind: EntryKind::Directory });
            continue;
        }
        let mut bytes = Vec::with_capacity(size as usize);
        entry.read_to_end(&mut bytes)?;
        let kind = if looks_like_xml(&bytes) {
            let (root, elements) =
                xml_structure(&bytes).map_err(|message| ImportError::Xml { entry: name.clone(), message })?;
            EntryKind::Xml { root, elements }
        } else {
            EntryKind::Binary { magic: sniff(&bytes).to_owned() }
        };
        entries.push(EntryInfo { name, size, kind });
    }
    Ok(Inspection { entries })
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

fn looks_like_xml(bytes: &[u8]) -> bool {
    strip_bom(bytes).iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<')
}

fn sniff(bytes: &[u8]) -> &'static str {
    const SIGNATURES: &[(&[u8], &str)] = &[
        (b"\x89PNG", "PNG image"),
        (b"\xFF\xD8\xFF", "JPEG image"),
        (b"GIF8", "GIF image"),
        (b"RIFF", "RIFF (WAV/WebP/AVI)"),
        (b"ID3", "MP3 audio"),
        (b"PK\x03\x04", "ZIP archive"),
        (b"%PDF", "PDF"),
        (b"OggS", "Ogg"),
        (b"wOFF", "WOFF font"),
        (b"\x00\x01\x00\x00", "TrueType font"),
        (b"OTTO", "OpenType font"),
    ];
    if bytes.len() > 8 && &bytes[4..8] == b"ftyp" {
        return "MP4/MOV video";
    }
    SIGNATURES.iter().find(|(sig, _)| bytes.starts_with(sig)).map_or("unknown", |(_, name)| name)
}

fn xml_structure(bytes: &[u8]) -> Result<(String, BTreeMap<String, ElementStats>), String> {
    let mut reader = quick_xml::Reader::from_reader(BufReader::new(strip_bom(bytes)));
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut elements: BTreeMap<String, ElementStats> = BTreeMap::new();
    let mut root = String::new();

    loop {
        let event = reader.read_event_into(&mut buf).map_err(|e| e.to_string())?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let name = e.name().as_ref().to_owned();
                if stack.is_empty() && root.is_empty() {
                    root = name.clone();
                }
                stack.push(name);
                let stats = elements.entry(stack.join("/")).or_default();
                stats.count += 1;
                for attr in e.attributes().flatten() {
                    let key = attr.key.as_ref().to_owned();
                    let mut value = attr.value.to_string();
                    if value.chars().count() > MAX_SAMPLE_LEN {
                        value = value.chars().take(MAX_SAMPLE_LEN).collect::<String>() + "…";
                    }
                    let samples = stats.attributes.entry(key).or_default();
                    if samples.len() < MAX_SAMPLES {
                        samples.insert(value);
                    }
                }
                if matches!(event, Event::Empty(_)) {
                    stack.pop();
                }
            }
            Event::End(_) => {
                stack.pop();
            }
            Event::Text(ref t) if !stack.is_empty() => {
                if !AsRef::<str>::as_ref(t).trim().is_empty() {
                    if let Some(stats) = elements.get_mut(&stack.join("/")) {
                        stats.has_text = true;
                    }
                }
            }
            Event::CData(_) if !stack.is_empty() => {
                if let Some(stats) = elements.get_mut(&stack.join("/")) {
                    stats.has_text = true;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok((root, elements))
}

impl fmt::Display for Inspection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} entries", self.entries.len())?;
        for entry in &self.entries {
            match &entry.kind {
                EntryKind::Directory => writeln!(f, "\n[dir] {}", entry.name)?,
                EntryKind::Binary { magic } => writeln!(f, "\n[bin] {} ({} bytes, {magic})", entry.name, entry.size)?,
                EntryKind::Xml { root, elements } => {
                    writeln!(f, "\n[xml] {} ({} bytes, root <{root}>)", entry.name, entry.size)?;
                    for (path, stats) in elements {
                        let depth = path.matches('/').count();
                        let leaf = path.rsplit('/').next().unwrap_or(path);
                        let text = if stats.has_text { " +text" } else { "" };
                        writeln!(f, "  {:indent$}<{leaf}> ×{}{text}", "", stats.count, indent = depth * 2)?;
                        for (attr, samples) in &stats.attributes {
                            let samples: Vec<_> = samples.iter().map(|s| format!("{s:?}")).collect();
                            writeln!(f, "  {:indent$}  @{attr} = {}", "", samples.join(" | "), indent = depth * 2)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
