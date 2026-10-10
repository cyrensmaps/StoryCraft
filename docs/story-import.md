# Importing Storyline projects

Opening `.story` files is a core goal. The importer is built from sample projects, following the [clean-room policy](clean-room.md). This page explains how to help.

## Current state

- `storycraft-cli inspect file.story` reports a `.story` archive's structure: every entry, its type, and for XML entries every element path with attribute names and a few sample values.
- `storycraft_story::import` and the editor's **File → Import Storyline project** report that the format is not mapped yet.

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
| 09 | `09-variables.story` | A number variable, a button that adds 1, and a text box showing `%Var%` |
| 10 | `10-conditions.story` | A trigger with two conditions joined by AND, and one joined by OR |
| 11 | `11-timeline.story` | Two objects that appear at 1 s and 2 s, and one that disappears at 3 s |
| 12 | `12-scenes.story` | Two scenes with two slides each |

For each sample, note anything you set that isn't visible in the file name (exact colours, sizes, times).

## Sharing

Run `storycraft-cli inspect <file>.story > <file>.txt` for each sample and share the `.txt` reports in an issue. If you are willing to license the sample `.story` files under MIT OR Apache-2.0, attach them too; they become test fixtures under `crates/storycraft-story/tests/fixtures/`.

## Mapping plan

Import will map, in order: project and slide size → scenes and slides → text, shapes and images → layers → triggers → states → variables and conditions → timing. Anything that cannot be represented yet is skipped with a warning listed after import, never silently dropped.
