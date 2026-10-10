# Architecture

## Crates

```
storycraft-model ──► storycraft-engine ──► storycraft-player (wasm32)
       │                    │                     │ built by build.rs, embedded
       │                    ▼                     ▼
       ├──────────► storycraft-editor ◄── storycraft-export ──► storycraft-cli
       └──────────► storycraft-story ─────────────────────────► (editor, cli)
```

### `storycraft-model`

Plain data with serde: `Project → Scene → Slide → Layer → Object`, plus `Trigger`, `Variable` and `Asset`. A slide has a base layer and further layers that triggers show and hide. Behaviour is data too: a `Trigger` is an `Action`, the `Event` that fires it, and optional `ConditionClause`s.

`archive::Document` is the `.storycraft` file: a ZIP with `project.json` (pretty-printed, so diffs are readable) and `media/<asset file>`. `FORMAT_VERSION` guards against opening files from newer versions.

### `storycraft-engine`

`Engine` plays a project. It owns the session state (current slide, visible layers and their timeline positions, variable values, object states, visited slides, navigation history) and exposes:

- input: `tick(dt)`, `pointer_move/down/up`, `click(id)`, `key_pressed`, `next_slide`, `previous_slide`
- output: `frame()` (a renderer-neutral `Frame`: every visible object with its state applied and `%Variable%` text resolved) and `take_effects()` (open a URL, report completion, exit, slide changed)
- persistence: `snapshot()` and `restore()` for LMS suspend data

The rules are written down at the top of `crates/storycraft-engine/src/lib.rs`, with tests in `tests.rs`.

### `storycraft-player`

A `cdylib` wrapping the engine for `wasm32-unknown-unknown`, with a tiny C ABI (`sc_init`, `sc_tick`, `sc_pointer_*`, `sc_frame`, …). Strings cross as UTF-8 in linear memory and data as JSON, so no binding generator is needed.

### `storycraft-export`

`build.rs` compiles the player with the size-optimised `player` profile and embeds the `.wasm`. `build()` produces a package:

```
index.html
storycraft/player.js   DOM renderer, input, SCORM 1.2 bridge
storycraft/player.css
storycraft/engine.js   the wasm, base64-encoded (loads from file:// too)
storycraft/course.js   the project JSON
media/...
imsmanifest.xml        SCORM only
```

`player.js` keeps one DOM element per object (keyed by id, so focus survives updates), renders interactive objects as `<button>`s for keyboard and screen-reader users, and forwards pointer events in slide coordinates to the engine, which does all hit testing.

### `storycraft-editor`

eframe/egui. The UI edits `EditorApp::doc` directly. Undo works by snapshot: each frame the project is compared with its state before the UI ran, and edits coalesce until the pointer is released and input pauses (`history.rs`). Commands that replace the document or move through history run between frames so they are never recorded as edits.

`draw.rs` paints engine `Frame`s. The edit canvas builds a static frame with `Frame::for_editing`, and the preview asks a live `Engine` for one, so editing and preview share a renderer.

### `storycraft-story`

Clean-room `.story` import; see [story-import.md](story-import.md).
