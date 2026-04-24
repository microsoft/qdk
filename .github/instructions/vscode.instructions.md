---
applyTo: "source/vscode/**"
description: "VS Code extension for Q# and OpenQASM. Covers product architecture, build scripts, bundling, testing strategy, and key conventions for the source/vscode folder."
---

# Q# VS Code Extension (`source/vscode/`)

Dual-platform (Node.js desktop + browser/web) VS Code extension for Q# and OpenQASM. esbuild produces separate bundles per platform from one TS codebase — see `build.mjs` for all platform targets.

## Key dependency: `qsharp-lang`

The WASM-compiled Rust compiler is consumed as the `qsharp-lang` npm package from the **in-repo workspace** (`source/npm/qsharp`), not npmjs. It provides `ILanguageService`, `loadWasmModule()`, and the full compiler/simulator stack. Nearly every feature goes through this package.

## Build

From repo root with `build.py`:

```
python build.py --wasm --npm --vscode                  # Full pipeline
python build.py --vscode                               # TS/JS only (skip --wasm/--npm if Rust/npm unchanged)
python build.py --npm --vscode                          # Skip --wasm if Rust unchanged
python build.py --wasm --npm --vscode --no-check        # Skip lints
python build.py --wasm --npm --vscode --no-test         # Skip unit tests
python build.py --wasm --npm --vscode --integration-tests  # Include VS Code integration tests (off by default)
```

## npm scripts (run from `source/vscode/`)

| Task                  | Command                                    |
| --------------------- | ------------------------------------------ |
| Build (tsc + esbuild) | `npm run build`                            |
| Build watch           | `npm run build:watch`                      |
| Type-check only       | `npm run tsc:check`                        |
| Integration tests     | `npm test` or `npm test -- --suite=<name>` |
| Katas tests           | `npm run test:learning`                    |
| Launch dev VS Code    | `npm start`                                |

**Do not use `npx`** to invoke tools — use the npm scripts above instead.

## Testing

- **Framework**: Mocha + Chai via `@vscode/test-web` with Playwright (headless Chromium). Tests run in the real VS Code extension host — no mocking library. Do not add test dependencies.
- Suites live in `test/suites/` with `test-workspace/` fixture folders. Use helpers from `test/extensionUtils.ts`. Follow existing patterns.

## Conventions

- **No CommonJS-only deps** in the learning bundle (ESM).
- **Webview HTML**: `--vscode-*` CSS variables for theming, CSP-clean nonce-based scripts (no inline event handlers).
- **Chat skills/instructions** for end users live in `skills/` and `resources/chat-instructions/`. Keep `SKILL.md` in sync when changing user-visible MCP tool behavior.
