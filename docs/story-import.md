# Importing Storyline projects

Opening `.story` files is a core goal. The importer is built from sample projects, following the [clean-room policy](clean-room.md). This page explains how to help.

## Current state

`storycraft-cli convert course.story` and the editor's **File → Import Storyline project** convert:

- scenes and slides (names, slide size, advance mode, timeline length)
- layers (name, timeline length, and the options "hide other slide layers", "pause base layer", "prevent clicking on the base layer" and "hide when the timeline finishes")
- text boxes, rectangles, rounded rectangles, ellipses, triangles, buttons, pictures and web objects (web addresses and local folders, shown in the slide or a new window), with position, fill and outline (including theme colours), text and alt text
- text formatting: alignment, font (including the theme's fonts), size, bold, italic and colour (one style per text box)
- object timing (start, duration, "show until end") and "initially hidden"
- states, including built-in button states and colours defined relative to Normal
- user variables (number, true/false, text)
- triggers: jump to next, previous or a specific slide; show and hide layer; change state; adjust variable
- trigger events: click, hover, timeline starts and ends, key press, variable changes
- conditions on variables and object states, joined by AND/OR

Anything not converted is listed as a note after import. Known gaps:

- gradients, other shape types, audio and video, quizzes

The structure that has been mapped is documented at the top of `crates/storycraft-story/src/import.rs`.

## Making sample projects

Build each sample in Storyline 360 with **as little as possible in it**, so differences between samples point straight at the feature. Use plain, made-up text, no media you don't own, and the default player.

| # | File name | Contents |
|---|---|---|
| 01 | `01-blank.story` | A new project, saved immediately |
| 02 | `02-two-slides.story` | Two blank slides, renamed "First" and "Second" |
| 03 | `03-text.story` | One text box reading `Hello`, at a known position and size |
| 04 | `04-shapes.story` | A rectangle, an ellipse and a triangle with distinct fill colours |
| 05 | `05-image.story` | One small picture you own (a 10×10 PNG is ideal) |
| 06 | `06-button-jump.story` | A button with a "Jump to next slide" trigger, and a second slide |
| 07 | `07-layer.story` | A button that shows a layer; the layer has a Close button that hides it |
| 08 | `08-states.story` | A shape with Hover and a custom state, plus a trigger that changes state |
| 09 | `09-variables.story` | A Number variable named `Counter` (initial value 0), a button whose trigger adds 1 to it, and a text box reading `Count: %Counter%` (note whether you typed the reference or used Insert → Reference) |
| 10 | `10-conditions.story` | A trigger with two conditions joined by AND, and one joined by OR |
| 11 | `11-timeline.story` | Two objects that appear at 1 s and 2 s, and one that disappears at 3 s |
| 12 | `12-scenes.story` | Two scenes with two slides each |

For each sample, note anything you set that isn't visible in the file name (exact colours, sizes, times).

### Quiz samples

Use Storyline's own question slides (Insert Slide → Quizzing → Graded) with made-up text. Note the points, attempts and feedback settings you used.

| # | File name | Contents |
|---|---|---|
| 23 | `23-quiz-multiple-choice.story` | One multiple-choice question: 3 choices, the 2nd correct, 10 points, 2 attempts, default feedback layers |
| 24 | `24-quiz-true-false.story` | One true/false question, "False" correct |
| 25 | `25-quiz-multiple-response.story` | One multiple-response question: 4 choices, the 1st and 3rd correct |
| 26 | `26-quiz-fill-in.story` | One fill-in-the-blank question with two accepted answers, e.g. "Paris" and "paris, france" |
| 27 | `27-quiz-matching.story` | One matching drop-down question with 3 pairs |
| 28 | `28-quiz-results.story` | Questions 23 and 24 plus a results slide with passing score 80%, and the course set to report pass/fail |


## Sharing

Run `storycraft-cli inspect <file>.story > <file>.txt` for each sample and share the `.txt` reports in an issue. If you are willing to license the sample `.story` files under MIT OR Apache-2.0, attach them too; they become test fixtures under `crates/storycraft-story/tests/fixtures/`.

## Mapping plan

Import will map, in order: project and slide size → scenes and slides → text, shapes and images → layers → triggers → states → variables and conditions → timing. Anything that cannot be represented yet is skipped with a warning listed after import, never silently dropped.
