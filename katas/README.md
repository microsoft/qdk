# Katas

A Kata is a top-level container of educational items which are used to explain a particular quantum computing topic using Q#. They are organized in sections which can be of three types:

- Lessons: text content that sometimes contains Q# code examples.
- Exercises: problems that the user solves by writting Q# code.
- Questions: analytical problems that have a text answer.

## Run Katas Online

Visit [Learn with Microsoft Quantum katas](https://quantum.microsoft.com/experience/quantum-katas) to try the new online Microsoft Quantum katas experience, with integrated assistance from Copilot in Microsoft Quantum.

## Validate Katas Locally

The Quantum Katas learning experience is part of the VS Code extension. To validate kata content locally:

1. From the repository root, build the extension and its dependencies with `python ./build.py --wasm --npm --vscode`.
2. In VS Code, press F5 (or select **Run > Start Debugging**) to launch the extension development host.
3. Open the QDK Learning experience and select the built-in Quantum Katas course.

## Rust crate

The katas crate exposes an API to check solutions for exercises.

## Composition

Katas are composed through an index.md markdown file that uses macros to indicate how to produce content for different kinds of sections.

### Macros

Macros are meant to insert interactive elements into the content defined through markdown. A macro starts with the `@` character followed by a word inside square brackets where the word represents the name of the macro (e.g. `@[example]`). This is followed by a JSON string enclosed within parenthesis `({...})` where the JSON string represents the properties of the macro that determine the interactive content. The macro is terminated by a line break `\r?\n`.

The following macros are available for katas composition:

- @[exercise]: Used to create Q# code exercises that we can be automatically verified.
  - id: Unique identifier for the exercise.
  - title: Title that will be displayed for the exercise.
  - path: Path to a folder that contains the description of the exercise. This folder should contain the following files:
    - `index.md`: the Markdown description of the exercise.
    - `Placeholder.qs`: the Q# code that is given to the learner to start with.
    - `Verification.qs`: the Q# code that checks whether the learner's solution is correct.
    - `solution.md`: the Markdown description of the solution(s) to the exercise.
    - `Solution.qs`: the Q# code that contains a "reference solution" described in `solution.md`.
    The @EntryPoint operation is called to check the solution (eventually, for the convention is to call Kata.Verification.CheckSolution).
- @[example]: Standalone Q# code snippets that can be referenced from markdown files.
  - id: Unique identifier for the example.
  - codePath: Path to a Q# file that contains the example code.
- @[solution]: represents a solution to a Q# code exercise. It is meant to be compiled as if it was the user authored code that solves a Q# code exercise. It can only be used in solution markdown files.
  - id: Unique identifier for the solution.
  - codePath: Path to a Q# file that contains the solution code.
- @[section]: A kata is broken into multiple sections. This starts a new section. Exercises are their own sections.
  - id: Unique identifier for the section.
  - title: Title of the section.

### Images

Constructing SVG images correctly for the katas is a little involved due to the need to support dark
and light themes on QCOM. Currently this follows the OS preference, but it is also common to allow the
theme to be set via a `data-theme` attribute on a parent element, (e.g., the playground does this to force
light mode, as it doesn't support other themes currently).

To support this, CSS needs to be used. However if the CSS is not contained within the image itself, then
editing or previewing the SVG file becomes very awkward. Therefore the approach taken is that the CSS
be included directly in each image. The below CSS should be pasted directly into each `.svg` file used
in the katas, directly after the opening `<svg>` element.

IMPORTANT: As the SVG file becomes inline HTML in Markdown, it is important that there are no blank lines
within the .svg files anywhere (including the CSS block), due to the way Markdown parsers determine
a block of HTML has terminated. The kata generation script checks for this and will fail if any exist.

```html
<style>
/* Default values, or theme set explicitly to light */
:root, [data-theme="light"] {
    --kata-svg-stroke: #222;
    --kata-svg-fill: #fff;
    --kata-svg-path: #777;
    --kata-svg-accent: #06c;
}
/* User has set OS preference for dark. (An explict light theme will override) */
@media(prefers-color-scheme: dark) {
    :root {
        --kata-svg-stroke: #eee;
        --kata-svg-fill: #111;
        --kata-svg-path: #bbb;
        --kata-svg-accent: #08f;
    }
}
/* Explicit dark theme set (should match above dark preference values) */
[data-theme="dark"] {
    --kata-svg-stroke: #eee;
    --kata-svg-fill: #111;
    --kata-svg-path: #bbb;
    --kata-svg-accent: #08f;
}
/*** Kata specific styles ***/
.kata_svg_path {
    stroke: var(--kata-svg-path);
    stroke-width: 2;
    stroke-linecap: round;
    fill: none;
}
.kata_svg_text {
    fill: var(--kata-svg-stroke);
}
.kata_svg_point {
    fill: var(--kata-svg-fill);
    stroke: var(--kata-svg-stroke);
}
.kata_svg_fill_accent {
    fill: var(--kata-svg-accent);
}
.kata_svg_stroke_accent {
    stroke: var(--kata-svg-accent);
}
</style>
```

For previewing how each SVG image will look, and for making manual tweaks to the SVG markup (which is
similar to HTML), I recommend the VS Code extension at <https://marketplace.visualstudio.com/items?itemName=jock.svg>.
With this installed in VS Code, you can right-click on an SVG file and select 'Preview SVG'. The CSS takes
effect, so changing the OS theme will change how the preview appears. (Note: You can choose a different background
color from the swatch at the top of the preview. I recommend the first swatch option of "Use editor background", and then enable the VS Code setting `Window: Auto-detect color scheme`. That way as you change OS theme, the VS Code preview will match).

For more involved SVG editing I recommend Inkscape (<https://inkscape.org/>), however it has a bit of
learning curve, and doesn't handle custom CSS properties very well, so the below CSS may need to be
modified before working in Inkscape, then replaced again after the image changes have been saved.

If not familiar with the SVG format, the MDN reference at <https://developer.mozilla.org/en-US/docs/Web/SVG/Tutorial>
provides a very good overview.

## Acknowledgements

Content of these web-based katas is largely a port of the previous effort located in the [QuantumKatas](https://github.com/microsoft/QuantumKatas) repository. Please refer to that repository for a history of contributions.
