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
storycraft/player.js   DOM renderer and input
storycraft/lms.js      LMS adapters: SCORM 1.2, SCORM 2004, xAPI, cmi5
storycraft/player.css
storycraft/engine.js   the wasm, base64-encoded (loads from file:// too)
storycraft/course.js   the project JSON
media/...
imsmanifest.xml        SCORM 1.2 / 2004
tincan.xml             xAPI
cmi5.xml               cmi5
```

`lms.js` gives the player one interface for every format (saved progress, save, slide viewed, complete, finish). SCORM adapters talk to the LMS's JavaScript API; the xAPI adapter reads the Tin Can launch parameters and sends statements (initialized, experienced per slide, completed, terminated) and progress (State API) to the LRS; the cmi5 adapter follows the cmi5 launch sequence (fetch the auth token, read `LMS.LaunchData`, send cmi5-defined statements using the context template, return to `returnURL`).

`player.js` keeps one DOM element per object (keyed by id, so focus survives updates), renders interactive objects as `<button>`s for keyboard and screen-reader users, and forwards pointer events in slide coordinates to the engine, which does all hit testing.

### `storycraft-editor`

eframe/egui. The UI edits `EditorApp::doc` directly. Undo works by snapshot: each frame the project is compared with its state before the UI ran, and edits coalesce until the pointer is released and input pauses (`history.rs`). Commands that replace the document or move through history run between frames so they are never recorded as edits.

`draw.rs` paints engine `Frame`s for the edit canvas (`Frame::for_editing`).

The preview (`web_preview.rs`) publishes the course to memory as an HTML package and shows it in a `wry` child web view (WebView2 / WKWebView) over the central panel, served through a `storycraft://` custom protocol. Learners and authors therefore see the same player, including web objects. The web view is a native window drawn above egui, so it is hidden while a menu or dialog is open. Where no web view is available (Linux for now, or if it fails to start) `preview.rs` drives the engine directly and paints with `draw.rs`.

On Windows with the GNU toolchain, WebView2 links against `WebView2Loader.dll`; the editor's `build.rs` copies it next to the executable. MSVC builds link it statically.

### `storycraft-story`

Clean-room `.story` import; see [story-import.md](story-import.md).
