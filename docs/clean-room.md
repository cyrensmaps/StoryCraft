# Clean-room policy

StoryCraft can read Articulate Storyline projects and offers similar authoring concepts, but it is an independent implementation. To keep the project legally safe for everyone who uses and contributes to it, these rules apply to all contributions.

## Never

- Decompile, disassemble or debug Storyline, its player or any other Articulate software, or use output from someone who did. Articulate's license prohibits this, and code derived that way would taint the project.
- Copy code, scripts, stylesheets, icons, fonts, templates or other assets from Storyline, from its installation folder, or from published Storyline output (`story.html`, `html5/lib/...`).
- Commit `.story` files, published Storyline output, or excerpts from either, unless you made them yourself and are entitled to license them under this project's licenses.
- Use Articulate's trademarks in a way that suggests StoryCraft is an Articulate product or endorsed by Articulate.

## Allowed

- Public documentation, tutorials and help articles, used to understand **what** a feature does. The implementation is our own.
- `.story` files you created and own, examined with `storycraft-cli inspect` to learn their **structure** (element names, attributes, how files relate). This is interoperability work on your own data.
- Observing Storyline's behaviour as a user (what happens when a trigger fires, the order layers draw in) and writing it down as a behavioural description.
- Open standards: SCORM, xAPI, cmi5, AICC, HTML, CSS, WebAssembly.

## How `.story` import is developed

1. Someone with a Storyline license builds small, purpose-made sample projects. Each one isolates a single feature (one text box; one trigger; one layer).
2. They run `storycraft-cli inspect` on each sample and share the structural reports (or, if they are willing to license them, the sample files themselves).
3. Importer code is written from those reports and from the behaviour the author describes, never from Storyline's code.

If you are unsure whether a contribution is acceptable, open an issue and ask before submitting it.
