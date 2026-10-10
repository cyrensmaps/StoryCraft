# StoryCraft

An open-source e-learning authoring tool written in Rust. StoryCraft lets you build interactive, slide-based courses (slides, layers, states, triggers, variables and timelines) and publish them as HTML5 websites or SCORM packages for any LMS.

StoryCraft is inspired by tools like Articulate Storyline 360 and aims to open Storyline projects, but it is an independent, clean-room implementation. It contains no Articulate code and is not affiliated with or endorsed by Articulate. See [docs/clean-room.md](docs/clean-room.md).

## Status

Early development (v0.1). What works today:

| Area | Status |
|---|---|
| Desktop editor (pure Rust, egui) | Scenes and slides, layers, text, shapes, buttons and pictures, move and resize, properties, undo/redo, copy/paste |
| Interactivity | Triggers (jump, show/hide layer, change state, set variable, timeline control, open URL, complete/exit), conditions with AND/OR, built-in Hover, Down, Visited, Selected, Disabled and Hidden states, custom states, `%Variable%` text |
| Timeline | Per-layer timelines, timed entrances and exits, automatic slide advance |
| Preview | In-app preview using the same engine as published courses |
| Publishing | HTML5 (runs from a web server or straight from disk) and SCORM 1.2 (completion, bookmarking, resume) |
| Storyline `.story` import | **In progress.** The inspection tooling is done; the format mapping is being built from sample files. See [docs/story-import.md](docs/story-import.md) |

Planned next: quizzes and result slides, audio and video, motion paths and animations, slide masters, SCORM 2004, xAPI and cmi5, and rich text.

## Building

You need a stable Rust toolchain ([rustup.rs](https://rustup.rs)) and the WebAssembly target, which is used to build the course player:

```sh
rustup target add wasm32-unknown-unknown
cargo run --release -p storycraft-editor            # the editor
cargo run --release -p storycraft-editor -- my.storycraft
```

**Windows:** either install the Visual Studio C++ Build Tools and use the default MSVC toolchain, or use the GNU toolchain (`x86_64-pc-windows-gnu`) with a full MinGW-w64 install on `PATH` (for example [WinLibs](https://winlibs.com): `winget install BrechtSanders.WinLibs.POSIX.UCRT`).

**Linux:** eframe needs a few system libraries, for example on Debian/Ubuntu: `sudo apt install libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libgtk-3-dev`.

## Command line

`storycraft-cli` publishes and checks projects without the editor:

```sh
cargo run -p storycraft-cli -- sample demo.storycraft                         # write a demo course
cargo run -p storycraft-cli -- validate demo.storycraft                       # check for broken references
cargo run -p storycraft-cli -- publish demo.storycraft -f scorm12 -o demo.zip # SCORM 1.2 package
cargo run -p storycraft-cli -- publish demo.storycraft -f html -o site/       # website
cargo run -p storycraft-cli -- inspect course.story                           # describe a .story file's structure
```

## How it fits together

```
crates/
  storycraft-model    the document model and the .storycraft file format (a ZIP of JSON and media)
  storycraft-engine   the runtime: timelines, triggers, conditions, variables, states
  storycraft-player   the engine compiled to WebAssembly for published courses
  storycraft-export   HTML5 and SCORM 1.2 publishing (embeds the player)
  storycraft-story    clean-room import of Storyline .story files
  storycraft-editor   the desktop app
  storycraft-cli      command-line tools
```

The editor preview and published courses both run `storycraft-engine`: the editor directly, the browser through WebAssembly. The JavaScript in a published course only draws the frames the engine produces and talks to the LMS, so a course behaves the same in preview and in the LMS. More detail in [docs/architecture.md](docs/architecture.md).

## Testing

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
```

`tools/browser-smoke.mjs` plays a published copy of the sample course in a headless Chromium-based browser and checks layers, variables, timing and SCORM reporting against a fake LMS:

```sh
cargo run -p storycraft-cli -- sample demo.storycraft
cargo run -p storycraft-cli -- publish demo.storycraft -f scorm12 -o out/demo
node tools/browser-smoke.mjs out/demo        # set BROWSER=/path/to/chrome off Windows
```

## Contributing

Contributions are welcome. Please read [docs/clean-room.md](docs/clean-room.md) first: StoryCraft must stay free of Articulate's code and copyrighted material.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
