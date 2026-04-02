# Q# Python Package — API Documentation

This directory contains a [Sphinx](https://www.sphinx-doc.org/) project that
auto-generates HTML API reference docs from the Python docstrings in the
`qsharp` package.

## Prerequisites

Install Sphinx, the theme, and any needed extensions into your Python
environment (a virtual environment is recommended):

```bash
pip install -r requirements.txt
```

## Building the HTML docs

**Windows:**
```bat
make.bat html
```

**Linux / macOS:**
```bash
make html
```

The generated site will be at `_build/html/index.html`.

## Live-reload server (dev convenience)

Install `sphinx-autobuild` and run:

```bash
pip install sphinx-autobuild
make livehtml          # Linux/macOS
sphinx-autobuild . _build/html   # Windows
```

This starts a local server at http://127.0.0.1:8000 that rebuilds whenever
you edit a source file.

## Adding new modules to the reference

1. Create a new `.rst` file under `api/` following the pattern of the existing
   files (e.g. `api/qsharp.estimator.rst`).
2. Add an `automodule` or individual `autoclass` / `autofunction` directives
   pointing at the Python module.
3. Reference the new file in `api/index.rst` under the `toctree`.

## Notes on compiled (Rust) extensions

The `qsharp._native` and `qsharp.noisy_simulator._noisy_simulator` modules are
compiled Rust extensions. Sphinx does not need to import them to build the docs
because they are listed in `autodoc_mock_imports` inside `conf.py`. This means:

- All pure-Python docstrings are picked up automatically.
- Types imported *from* the native extension (e.g. `TargetProfile`, `Result`,
  `Pauli`) appear in the rendered docs via their stub file (`_native.pyi`).
  If a type's docstring is missing in the rendered output, add it to the
  corresponding `.pyi` file and Sphinx will pick it up.

## Deploying to GitHub Pages or Read the Docs

- **GitHub Pages**: add a CI step that runs `make html` and pushes `_build/html`
  to the `gh-pages` branch (see GitHub Actions `peaceiris/actions-gh-pages`).
- **Read the Docs**: add a `.readthedocs.yaml` at the repo root pointing at this
  directory as the Sphinx configuration directory.
