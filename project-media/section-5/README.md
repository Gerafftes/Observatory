# Recorded presentation graphics

These are editable SVG/PNG illustrations and narration for the recorded
project presentation, not new measurement results. The source references
are embedded in the SVG metadata.

To regenerate with Node.js, run `npm install --ignore-scripts` followed by
`npm run render` in this directory. The dependency on sharp is declared
in `package.json`; the script no longer relies on a private Codex runtime.
