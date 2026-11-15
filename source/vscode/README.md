# Microsoft Quantum Development Kit (QDK)

This extension brings rich language support for Q# and OpenQASM to VS Code. Develop, build, and run your quantum code from VS Code either locally on simulators, or by submitting a job to Azure Quantum.

You can also try out this extension in VS Code for Web at [vscode.dev/quantum/playground](https://vscode.dev/quantum/playground/).

## Features

The QDK extension currently supports:

- Syntax highlighting and syntax features (e.g. brace matching) for Q#, OpenQASM, and DEQ
- Editing features such as go-to-definition, suggestions and signature help for Q# and OpenQASM
- Error checking in Q# and OpenQASM source files
- Local quantum simulation, including support for Pauli noise and qubit loss
- Breakpoint debugging for Q# and OpenQASM
- Code samples for Q# and OpenQASM demonstrating well known algorithms
- Circuit diagram visualization and editing
- Q# cell support in Jupyter notebooks. The extension will detect `%%qsharp` magic cells and automatically update the cell language to Q#
- Integration with Azure Quantum for job submission to quantum hardware providers

For more information about the QDK and Microsoft Quantum, visit [https://aka.ms/qdk](https://aka.ms/qdk).

## Exporting circuit diagrams

Select **Export as SVG** in the circuit viewer to save a transparent vector
image using QDK's light-theme colors. The export preserves expanded and collapsed
operations and includes the full circuit, independently of the viewer's zoom.
The interactive viewer continues to use your current theme.

Labels remain editable SVG text. The SVG embeds QDK's existing KaTeX fonts for
standalone browsers. Inkscape and other converters that do not load SVG web
fonts need the matching `KaTeX_Main-Regular.ttf` and `KaTeX_Math-Italic.ttf`
fonts installed locally. These fonts are available in the
[KaTeX distribution](https://github.com/KaTeX/KaTeX/releases).
Without them, those tools substitute fonts and can change label spacing.

For LaTeX or Overleaf, convert the SVG locally with Inkscape, then include the
resulting vector PDF with `graphicx`:

```shell
inkscape circuit.svg --export-type=pdf --export-filename=circuit.pdf
```

```latex
\usepackage{graphicx}
% In the document:
\includegraphics[width=\linewidth]{circuit.pdf}
```

If using the LaTeX `svg` package instead, set `inkscapelatex=false` to keep the
circuit's labels rather than having LaTeX re-typeset them.

## Contributing

To log issues, contribute to the project, or build the extension yourself, visit the repository at <https://github.com/microsoft/qdk>

## Data and telemetry

This extension collects usage data and sends it to Microsoft to help improve our products and services.
Details of the telemetry sent can be seen in the source file at <https://github.com/microsoft/qdk/blob/main/source/vscode/src/telemetry.ts>.
This extension respects the `telemetry.enableTelemetry` setting which you can learn more about at
<https://code.visualstudio.com/docs/supporting/faq#_how-to-disable-telemetry-reporting>.
