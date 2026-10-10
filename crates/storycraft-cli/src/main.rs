use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use storycraft_export::Target;
use storycraft_model::archive::Document;

/// Command-line tools for StoryCraft projects.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Publish a project as a web or SCORM package.
    Publish {
        /// A .storycraft (or .story) project.
        project: PathBuf,
        #[arg(long, short, value_enum, default_value_t = Format::Html)]
        format: Format,
        /// Output folder, or a file ending in .zip.
        #[arg(long, short)]
        out: PathBuf,
    },
    /// Check a project for broken references.
    Validate { project: PathBuf },
    /// Describe the internal structure of a Storyline .story file, to help
    /// map the format for import.
    Inspect { story: PathBuf },
    /// Write the built-in sample course to a .storycraft file.
    Sample { out: PathBuf },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Html,
    Scorm12,
}

impl From<Format> for Target {
    fn from(f: Format) -> Self {
        match f {
            Format::Html => Target::Html,
            Format::Scorm12 => Target::Scorm12,
        }
    }
}

fn load(path: &Path) -> Result<Document> {
    let is_story = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("story"));
    if is_story {
        let imported = storycraft_story::import(path).with_context(|| format!("importing {}", path.display()))?;
        for w in &imported.warnings {
            eprintln!("import warning: {w}");
        }
        Ok(imported.document)
    } else {
        Document::load(path).with_context(|| format!("opening {}", path.display()))
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Publish { project, format, out } => {
            let doc = load(&project)?;
            let package = storycraft_export::build(&doc, format.into())?;
            for w in &package.warnings {
                eprintln!("warning: {w}");
            }
            package.write_to(&out).with_context(|| format!("writing {}", out.display()))?;
            println!("Published {} files to {}", package.files.len(), out.display());
        }
        Command::Validate { project } => {
            let doc = load(&project)?;
            let problems = doc.project.validate();
            if problems.is_empty() {
                println!("No problems found.");
            } else {
                for p in &problems {
                    println!("{p}");
                }
                bail!("{} problem(s) found", problems.len());
            }
        }
        Command::Inspect { story } => {
            let report =
                storycraft_story::inspect(&story).with_context(|| format!("inspecting {}", story.display()))?;
            print!("{report}");
        }
        Command::Sample { out } => {
            Document::new(storycraft_model::sample::sample_project())
                .save(&out)
                .with_context(|| format!("writing {}", out.display()))?;
            println!("Wrote {}", out.display());
        }
    }
    Ok(())
}
