---
applyTo: "**/*.{qs,qsc,ipynb}"
---

# Q# coding instructions

Follow these instructions when generating Q# code in .qs files,
and Q# project folders that include a `qsharp.json` file.

## Syntax

When writing Q#, the `for (i in 1..100)` syntax is invalid, use `for i in 1..100` or
`for element in array[2...]`.

While many Q# operators are C-like, it uses `or` instead of `||` and `and` instead of `&&`.

To extract values from a tuple, use destructuring via the `let (item0, item1) = tupleValue;` syntax.

## Project structure

### Single-file projects

Q# files don't always need to exist in a project. A single `.qs` file can be compiled and
run without a `qsharp.json` file. Prefer a single `.qs` file for simple programs.

### Multi-file projects

When Q# source files need to to reference each other, a `qsharp.json` file must be
created. Source files must exist under the `src` folder.

Example layout:

```
project_root
|--qsharp.json
|--src
|--|--Main.qs
|--|--Tests.qs
```

A typical `qsharp.json` will be a JSON file with an empty JSON object in it.

```json
{}
```

Q# best practice is to _NOT_ use `namespace` blocks to enclose code.
Each function or operation is in a namespace which is the name of the containing file.
For example, if `Main.qs` has an operation `Foo`, then `Tests.qs` could reference the
operation as `Main.Foo`, or bring `Foo` into scope by adding `import Main.Foo;` in the file.

## Testing

The Q# language supports unit testing in VS Code. To write a test, use the `@Test()`
attribute on an operation, and `fail` with a message on test failure, e.g.,

```qsharp
@Test()
operation MyTestCase() : Unit {
    let result = DoOp();
    if (result != Expected) {
        fail $"DoOp returned {result}"
    }
}
```

Note: Prefer using a conditional `fail` statement to `Fact` calls, as `fail` gives a better error location.

## Libraries

A Q# project can reference a library from GitHub by updating the `dependencies` entry of
the `qsharp.json` file. For example, to reference the `chemistry` library, the `qsharp.json`
file might appear as:

```json
{
  "dependencies": {
    "Chemistry": {
      "github": {
        "ref": "v1.15.0",
        "owner": "microsoft",
        "repo": "qsharp",
        "path": "library/chemistry"
      }
    }
  }
}
```

## Jupyter Notebooks

Q# has first-class support for Jupyter notebooks. Typically the first cell will contain `import qsharp`.

Jupyter cells can contain Q# code directly by using the `%%qsharp` magic command at the beginning of the cell. For example:

```python
%%qsharp

operation GHZSample(n: Int) : Result[] {
    use qs = Qubit[n];

    H(qs[0]);
    ApplyToEach(CNOT(qs[0], _), qs[1...]);

    let results = MeasureEachZ(qs);
    ResetAll(qs);
    return results;
}
```

The `qsharp_widgets` package provides viewers for circuits and histograms, e.g.

```python
from qsharp_widgets import Circuit, Histogram
Circuit(qsharp.circuit("GHZSample(3)"))
```

Note that the latest Q# and QDK releases don't require or use the old IQ# kernel. It just needs to the `qsharp` PyPI package,
and maybe `qsharp_widgets` for visuals.

## Setup and tools

The Quantum Development Kit (QDK) was re-written at the start of 2024 and no longer uses
the IQ# Jupyter kernel, or the `dotnet` command line tools. Job management is best handled
now via tool calls integration into GitHub Copilot, or via Python code using the `qsharp`
and `azure-quantum` packages.

To execute Q# code, use the provided tools.

Whenever the user asks about Q# standard libraries, their contents, or any Q# library function, you **must** call the `qsharpGetLibraryDescriptions` tool to retrieve the authoritative list of available Q# library items. When generating Q# code, always use the `qsharpGetLibraryDescriptions` tool to determine which library functions are available and to ensure you use them correctly in your code suggestions. Do not attempt to answer questions about Q# library APIs, functions, or operations without first consulting this tool.

## Response formatting

Avoid using LaTeX in your responses to the user.
