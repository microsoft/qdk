---
applyTo: "source/vscode/src/learning/**,source/vscode/src/gh-copilot/learningTools.ts,source/vscode/agents/qdk-learning.agent.md"
description: "Use when editing the Quantum Katas learning experience: LearningService, LM tools, webview panel, activity-bar progress tree, or the qdk-learning agent file."
---

# Key rules

1. **Keep the agent file in sync.** [`agents/qdk-learning.agent.md`](../../source/vscode/agents/qdk-learning.agent.md) documents LM tools for the QDK Learning agent. When changing anything user-visible in `learningTools.ts` or the panel, update the agent file in the same change.
2. **Detector is shared.** `learning/progress/detector.ts` is the single source of truth for workspace detection. It is consumed by `LearningTools.init()`, `KatasPanelManager`, and `ProgressWatcher`. Adding fields is fine; renaming or removing requires updating all three call sites.
3. **Visually verify panel changes.** When editing panel assets (`learning/panel/webview.css`, `render.js`), drive the running extension with browser tools (`open_browser_page`, `read_page`, etc.) to verify rendering.
4. **Telemetry convention.** Use `EventType.KatasPanelAction` with the `action` property (see `telemetry.ts`).

## Build and test

From `source/vscode`:

- `npm run build` — type-checks all tsconfigs and runs esbuild. Panel assets (CSS, JS) are copied by `build.mjs` → `copyLearningPanelAssets()`.

# Architecture overview

Everything runs **in-process** inside the extension host. No out-of-process servers.

```
extension.ts
  → LearningService(extensionUri)            # singleton, owns all state
  → registerLanguageModelTools(ctx, service)  # wraps service as vscode.lm tools
  → registerKatasProgressView(ctx)            # activity-bar tree + ProgressWatcher
  → registerLearningCommands(ctx, service, watcher)  # all learning + tree commands
  → registerKatasPanelCommand(ctx, watcher, service)  # full-size webview panel
```

## `src/learning/`

All learning feature code lives under a single `learning/` folder.

- **`service.ts`** — Singleton `LearningService` — core business logic, UI-agnostic. Owns position, progress, `.qs` file scaffolding. Uses `QscEventTarget` + `loadCompilerWorker` for run/circuit/check. Fires `onDidChangeState` after every mutation so UIs stay in sync.
- **`types.ts`** — Canonical type definitions shared by all learning modules (including `ProgressFileData`).
- **`commands.ts`** — All learning commands: editor-facing (`learningShowHint`, `learningResetExercise`, `learningNext`, `learningOpenPanel`) and activity-bar tree (`katasRefresh`, `katasContinue`, `katasOpenSection`, `katasAskInChat`).
- **`codeLens.ts`** — CodeLens provider for exercise files.
- **`decorations.ts`** — Placeholder highlighting and pass/fail flash.
- **`editorContext.ts`** — Context keys for editor toolbar visibility.

## `src/learning/panel/`

`KatasPanelManager` (singleton `WebviewPanel`). Bridges `postMessage` ↔ `LearningService`, opens the associated `.qs` file in a secondary editor column. CSS/JS assets in `webview.css`/`render.js`, copied to `out/learning/panel/` at build time.

## `src/learning/progress/`

Activity-bar sidebar — a native `TreeView` fed by `ProgressWatcher` over `qdk-learning.json`.

- **`detector.ts`** — scans workspace folders for `qdk-learning.json`, returns `KatasWorkspaceInfo`.
- **`progressReader.ts`** — `ProgressWatcher` watches the progress file, maintains `qsharp-vscode.katasDetected` context key.
- **`catalog.ts`** — loads kata list from `qsharp-lang/katas-md` (`getAllKatas()`). `RECOMMENDED_ORDER` controls display order.
- **`treeProvider.ts`** — renders kata → section nodes with a "continue" node. `contextValue`: `kata` | `section` | `continue`.
- **`types.ts`** — Progress-view-specific types (`CatalogSection`, `CatalogKata`, `SectionProgress`, `KataProgress`, `OverallProgress`). Re-exports `ProgressFileData` from `../types.js`.

## `src/gh-copilot/learningTools.ts`

`LearningTools` wraps `LearningService` as `vscode.lm` language-model tools (registered via `registerLanguageModelTools` in `tools.ts`). All methods return `{ result?, state }` with state serialized compactly (current kata only). Most methods auto-reveal the webview panel via `openPanel()`. Throws `CopilotToolError` for user-facing errors. `circuit()` and `estimate()` delegate to `QSharpTools`.
