# Building from Source

To install locally:

- Build the extension by running `build.py` or `build.py --wasm --npm --vscode` from the repo root.
- Package the `VSIX` with `vsce package` while in the `vscode` directory. To get `vsce`, run `npm install -g @vscode/vsce`
- In VS Code, run command "Extensions: Install from VSIX..."
- Select the `VSIX` you just packaged (`qsharp.vscode-0.0.0.vsix` for example) in the directory.
- Reload your VS Code window.

This will enable the extension for all instances of VS Code.

To scope the extension to only a specific workspace (for example, the `qsharp` repo):

- In VS Code, find and open the "Q# (new)" extension in the Extensions view.
- Click the "Disable" button to disable the extension globally.
- Click the dropdown next to "Enable" button and select "Enable (Workspace)".

This will enable the extension for only the current workspace. The extension will remain
enabled for that workspace across restarts.

# Debugging

The repo includes several launch configurations in `.vscode/launch.shared.json` for
debugging the extension. All configs use the `--profile=dev` flag to isolate the
Extension Development Host from your default VS Code profile.

- **Debug VS Code extension** — Launches a local Extension Development Host window with
  the extension loaded. Use this for local (non-remote) development.

- **Debug VS Code Extension (Codespaces)** — Launches an Extension Development Host that
  connects back to the current Codespace and loads the extension in the remote extension
  host. The debugger attaches to the remote extension host process, so breakpoints work.
  Requires the `GitHub Codespaces` extension to be installed in the `dev` profile.

- **Debug VS Code Extension (WSL)** — Same as above, but connects to the current WSL
  distro. Requires the `WSL` extension to be installed in the `dev` profile.

- **Debug VS Code Extension (Web)** — Launches the extension as a web extension. Use this
  to test the extension as it would run in vscode.dev or github.dev, where only the
  browser-side entry point (`browser` in `package.json`) is loaded.

## Profile setup

The `--profile=dev` flag uses a dedicated VS Code profile, which starts empty. For the
remote launch configs, you need to install the corresponding remote extension in that
profile:

- **Codespaces**: `code --profile=dev --install-extension github.codespaces`
- **WSL**: `code --profile=dev --install-extension ms-vscode-remote.remote-wsl`
