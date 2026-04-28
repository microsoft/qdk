---
applyTo: "source/vscode/src/learningService/**,source/vscode/src/katasPanel/**,source/vscode/src/gh-copilot/learningTools.ts,source/vscode/src/katasProgress/**,source/vscode/agents/qdk-learning.agent.md"
description: "Use when editing the Quantum Katas learning experience: LearningService, LM tools, webview panel, activity-bar progress tree, or the qdk-learning agent file."
---

# Key rules

1. **Keep the agent file in sync.** [`agents/qdk-learning.agent.md`](../../source/vscode/agents/qdk-learning.agent.md) documents LM tools for the QDK Learning agent. When changing anything user-visible in `learningTools.ts` or `katasPanel/`, update the agent file in the same change.
2. **Detector is shared.** `katasProgress/detector.ts` is the single source of truth for workspace detection. It is consumed by `LearningTools.init()`, `KatasPanelManager`, and `ProgressWatcher`. Adding fields is fine; renaming or removing requires updating all three call sites.
3. **Visually verify panel changes.** When editing `katasPanel/katas-webview.html` or `.css`, drive the running extension with browser tools (`open_browser_page`, `read_page`, etc.) to verify rendering.
4. **Telemetry convention.** Use `EventType.KatasPanelAction` with the `action` property (see `telemetry.ts`).

## Build and test

From `source/vscode`:

- `npm run build` — type-checks all tsconfigs and runs esbuild. Panel assets (HTML, CSS, JS) are copied by `build.mjs` → `copyKatasPanelAssets()`.

# Architecture overview

Everything runs **in-process** inside the extension host. No out-of-process servers.

```
extension.ts
  → LearningService(extensionUri)            # singleton, owns all state
  → registerLanguageModelTools(ctx, service)  # wraps service as vscode.lm tools
  → registerKatasProgressView(ctx)            # activity-bar tree + ProgressWatcher
  → registerKatasPanelCommand(ctx, watcher, service)  # full-size webview panel
```

## `src/learningService/`

Singleton `LearningService` — core business logic, UI-agnostic. Owns position, progress, `.qs` file scaffolding. Uses `QscEventTarget` + `loadCompilerWorker` for run/circuit/check. Fires `onDidChangeState` after every mutation so UIs stay in sync.

## `src/gh-copilot/learningTools.ts`

`LearningTools` wraps `LearningService` as `vscode.lm` language-model tools (registered via `registerLanguageModelTools` in `tools.ts`). All methods return `{ result?, state }` with state serialized compactly (current kata only). Most methods auto-reveal the webview panel via `openPanel()`. Throws `CopilotToolError` for user-facing errors. `circuit()` and `estimate()` delegate to `QSharpTools`.

## `src/katasPanel/`

`KatasPanelManager` (singleton `WebviewPanel`). Bridges `postMessage` ↔ `LearningService`, opens the associated `.qs` file in a secondary editor column. HTML/CSS/JS assets in `katas-webview.html`/`.css`/`render.js`, copied to `out/` at build time.

## `src/katasProgress/`

Activity-bar sidebar — a native `TreeView` fed by `ProgressWatcher` over `qdk-learning.json`.

- **`detector.ts`** — scans workspace folders for `qdk-learning.json`, returns `KatasWorkspaceInfo`.
- **`progressReader.ts`** — `ProgressWatcher` watches the progress file, maintains `qsharp-vscode.katasDetected` context key.
- **`catalog.ts`** — loads kata list from `qsharp-lang/katas-md` (`getAllKatas()`). `RECOMMENDED_ORDER` controls display order.
- **`treeProvider.ts`** — renders kata → section nodes with a "continue" node. `contextValue`: `kata` | `section` | `continue`.
- **`commands.ts`** — `katasRefresh`, `katasContinue`, `katasOpenSection`, `katasAskInChat`. Navigates the panel directly; falls back to chat with `/qdk-learning #goto`.
