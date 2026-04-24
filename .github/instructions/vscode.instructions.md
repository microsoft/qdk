---
applyTo: "source/vscode/**"
description: "VS Code extension for Q# and OpenQASM. Covers product architecture, build scripts, bundling, testing strategy, and key conventions for the source/vscode folder."
---

# Q# VS Code Extension (`source/vscode/`)

The extension provides Q# and OpenQASM language support: IntelliSense, debugging, circuit visualization, Jupyter notebook integration, Azure Quantum job submission, resource estimation, and interactive Quantum Katas.

## Architecture

The extension is **dual-platform** — it runs in both desktop (Node.js) and browser (VS Code for Web). esbuild produces separate bundles for each platform from a single TypeScript codebase.

### Entry point

`src/extension.ts` is the activation entry. It wires up all subsystems: language service, debugger, Azure workspaces, circuit viewer, notebooks, project system, Katas, and Copilot chat tools.

### Source layout (`src/`)

| Directory / File                 | Purpose                                                                                                                          |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `language-service/`              | LSP features: completions, hover, diagnostics, formatting, go-to-def, references, rename, code lens, code actions, test explorer |
| `debugger/`                      | DAP sessions for Q# and OpenQASM, breakpoint management                                                                          |
| `azure/`                         | Azure Quantum workspace tree, job submission, auth, result download                                                              |
| `learning/`                      | Katas MCP server, TUI, web UI _(has its own instructions file)_                                                                  |
| `katasProgress/`                 | Activity-bar Katas panel (webview + tree view) _(has its own instructions file)_                                                 |
| `gh-copilot/`                    | GitHub Copilot chat tools (`azureQuantum*`), chat instructions loader                                                            |
| `webview/`                       | React webview components: circuit viewer, help panel, doc viewer                                                                 |
| `circuit.ts`, `circuitEditor.ts` | Circuit generation, caching, and `.qsc` custom editor                                                                            |
| `compilerWorker.ts`              | Spawns worker thread for heavy compiler operations                                                                               |
| `projectSystem.ts`               | `qsharp.json` manifest parsing, file dependency resolution, GitHub source fetching                                               |
| `notebook.ts`                    | Q# cell detection in Jupyter notebooks, magic cell handling                                                                      |
| `run.ts`                         | Run/debug command handlers                                                                                                       |
| `qirGeneration.ts`               | QIR code generation                                                                                                              |
| `config.ts`                      | VS Code settings (`Q#.*`)                                                                                                        |
| `memfs.ts`                       | Virtual filesystem for `qsharp-vfs://` URI scheme                                                                                |
| `telemetry.ts`                   | Telemetry events                                                                                                                 |

### Key dependency: `qsharp-lang`

The WASM-compiled Rust compiler is consumed as the `qsharp-lang` npm package from the in-repo workspace (`source/npm/qsharp`), **not** from npmjs. It provides `ILanguageService`, `getLanguageService()`, `loadWasmModule()`, and the full compiler/simulator stack. Nearly every feature in the extension goes through this package.

To rebuild the WASM + npm package: `python build.py --wasm --npm` from the repo root.

### Build targets (defined in `build.mjs`)

| Platform key   | Output                     | Description                                            |
| -------------- | -------------------------- | ------------------------------------------------------ |
| `node`         | `out/node/extension.js`    | Desktop extension (CJS, Node.js)                       |
| `browser`      | `out/browser/extension.js` | Web extension (CJS, browser)                           |
| `ui`           | `out/`                     | React webview bundles (circuit viewer, help, docview)  |
| `node-worker`  | `out/`                     | Worker threads (compiler worker, debug service worker) |
| `learning-cli` | `out/learning/index.js`    | Katas MCP server CLI (ESM)                             |

## Build and dev workflow

All commands run from `source/vscode/`:

```bash
npm run build              # TypeScript check + esbuild production bundle
npm run build:watch        # Continuous esbuild watch for dev
npm run tsc:check          # TypeScript check across all three tsconfigs (main, webview, learning)
npm run start              # Launch VS Code for Web at localhost (loads extension + samples workspace)
```

### Full build from repo root

```bash
python build.py --wasm --npm --vscode   # Builds WASM → npm package → VS Code extension
python build.py --wasm --npm --vscode --no-check  # Skip lints and formatting checks
python build.py --wasm --npm --vscode --no-test   # Build without running unit tests
python build.py --wasm --npm --vscode --integration-tests  # Also run VS Code integration tests (not included by default)
```

### Incremental builds

Omit earlier pipeline stages if you didn't change their source:

- **Only touched TS/JS in `source/vscode/`?** → `python build.py --vscode` (skip `--wasm` and `--npm`)
- **Changed npm package JS but not Rust?** → `python build.py --npm --vscode` (skip `--wasm`)
- **Changed Rust compiler code?** → need the full `--wasm --npm --vscode` chain

Or skip `build.py` entirely and use `npm run build` from `source/vscode/` for the fastest TS-only iteration.

### Formatting

- **TypeScript/JavaScript**: `npm run prettier:fix` from repo root
- **Before PR**: run `cargo fmt` (Rust) and `npm run prettier:fix` (JS/TS). `build.py` without arguments must pass.

## Testing

### Framework

- **Test runner**: Mocha (via `@vscode/test-web`)
- **Assertions**: Chai
- **Browser automation**: Playwright (headless Chromium)
- **No mocking library** — tests run in the real VS Code extension host and interact with the VS Code API directly

### Test suites

Located in `test/suites/`:

| Suite              | Files                                          | What it tests                                         |
| ------------------ | ---------------------------------------------- | ----------------------------------------------------- |
| `language-service` | `language-service.test.ts`, `notebook.test.ts` | Completions, hover, diagnostics, go-to-def, notebooks |
| `debugger`         | `qsharp.test.ts`, `openqasm.test.ts`           | Q# and OpenQASM debugging scenarios                   |

Each suite has a `test-workspace/` folder with fixture files.

### Running tests

```bash
npm test                                # All suites
npm test -- --suite=language-service    # Single suite
npm test -- --suite=debugger --waitForDebugger=1234  # Attach debugger
npm run test:learning                   # Katas server tests (node:test runner, separate from mocha)
```

### Test conventions

- Tests use helpers in `test/extensionUtils.ts` for common operations (opening docs, waiting for diagnostics, etc.)
- Do not add new dependencies to the test suite
- Follow existing test patterns in each suite file — many tests use shared helpers for brevity

## Conventions

- **Do not add CommonJS-only dependencies** to the learning bundle (ESM).
- **Webview HTML** uses `--vscode-*` CSS variables for theming and CSP-clean nonce-based scripts (no inline event handlers).
- **Settings** live under the `Q#.*` namespace. See `package.json` `contributes.configuration`.
- **Activation events**: `onUri`, `onNotebook:jupyter-notebook`, `onDebug`, `onDebugResolve:qsharp`, `onFileSystem:qsharp-vfs`, `onWebviewPanel:qsharp-webview`.
- **File types**: `.qs` (Q#), `.qsc` (circuit editor), `.qasm`/`.inc` (OpenQASM), `qsharp.json` (project manifest), `.ipynb` (notebooks with Q# cells).
- **Chat skills and instructions** for end users are in `skills/` and `resources/chat-instructions/`. Keep `SKILL.md` in sync when changing user-visible MCP tool behavior.
