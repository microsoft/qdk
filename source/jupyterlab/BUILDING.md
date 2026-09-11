# Q# extension for JupyterLab

## Building

Run `pip install .` from the `jupyterlab` directory to build the extension.

## Installing

You will need to install `jupyterlab` to test the extension and
`jupyter-builder` to rebuild its frontend assets directly.

```bash
pip install jupyterlab jupyter-builder
```

To install the extension in development mode, from the `jupyterlab` directory run:

```bash
pip install -e .
# The below command creates a symlink from JupyterLab's
# extensions directory to the current source directory.
jupyter labextension develop . --overwrite
```

To start JupyterLab and use the extension, run:

```bash
jupyter lab
```

To rebuild after making source changes, run:

```bash
npm run build
```

Then refresh the browser.

## Uninstalling

Remove the `pip` package by running:

```bash
pip uninstall qsharp-jupyterlab
```

You will also need to remove the symlink created by `jupyter labextension develop`
command. To find its location, you can run `jupyter labextension list` to figure out where the `labextensions` folder is located. Then you can remove the symlink named `qsharp-jupyterlab` within that folder.
