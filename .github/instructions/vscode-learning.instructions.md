---
applyTo: "source/vscode/src/learning/**,source/vscode/src/katasMcp.ts,source/vscode/src/katasProgress/**,source/vscode/agents/qdk-learning.agent.md"
description: "Q# Quantum Katas MCP server, TUI, web UI, and progress activity-bar panel embedded in the VS Code extension."
---

# source/vscode/src/learning

This folder builds a single Node bundle (`out/learning/index.js`) that provides four front-ends over one shared `KatasServer`:

- **MCP stdio** — `node out/learning/index.js --mcp`. Launched by the extension via `vscode.lm.registerMcpServerDefinitionProvider` (see [`src/katasMcp.ts`](../../source/vscode/src/katasMcp.ts)).
- **MCP HTTP** — `node out/learning/index.js --mcp-http --port <P>` (Streamable HTTP at `/mcp`). Used for smoke tests.
- **Web UI** — `node out/learning/index.js --web --port <P>` serves the standalone app at `/` and an MCP-widget preview at `/widget`.
- **TUI** — `node out/learning/index.js` (interactive terminal; requires a real TTY).

Use `--workspace <path>` to control where exercise `.qs` files and `qdk-learning.json` are scaffolded. Without it, the CWD is used (gitignored repo-wide).

## Build and test

From `source/vscode`:

- `npm run build` — type-checks all three tsconfigs and runs esbuild's `learning-cli` platform. Asset copy (widget HTML, `web/public/`) happens in `build.mjs` → `copyLearningAssets()`.
- `npm run test:learning` — node:test over `tests/server.test.ts` (31 tests).
- **Restart the web server after every rebuild.** It caches the inlined widget HTML in memory on first request.

## Runtime dependencies

- `qsharp-lang` is consumed from the in-repo workspace (`source/npm/qsharp`), **not** npmjs. `server/compiler.ts` explicitly calls `loadWasmModule()` on `qsharp-lang/lib/nodejs/qsc_wasm_bg.wasm` before `getCompiler()`.
- The bundle is ESM (`src/learning/package.json` + `out/learning/package.json` both set `"type": "module"`). Do not introduce CommonJS-only dependencies.

## Path resolution

`__dirname` differs between dev (tsx from source) and the bundle (`out/learning/`). When loading assets, probe the bundled layout first and fall back to the source layout:

```ts
const sharedDir = existsSync(join(__dirname, "web", "public", "shared"))
  ? join(__dirname, "web", "public", "shared")
  : join(__dirname, "..", "web", "public", "shared");
```

Bundled layout: `out/learning/{index.js,widget/app.html,web/public/**}`.

## UI changes need visual verification

When editing anything under `web/public/` or `mcp/widget/`, drive the running web server with the browser tools (`open_browser_page`, `read_page`, `click_element`, `run_playwright_code`). Prefer element refs from `read_page` over CSS selectors. For the TUI, a launch smoke test is enough — stdin is hard to automate.

## Keep the agent in sync

[`source/vscode/agents/qdk-learning.agent.md`](../../source/vscode/agents/qdk-learning.agent.md) documents the MCP tools for the QDK Learning agent. When you change anything user-visible in `mcp/server.ts` or `mcp/widget/app.html`, update the agent file in the same change.

# source/vscode/src/katasProgress

The activity-bar **Quantum Katas** panel: a `WebviewView` overview on top of a native `TreeView` of katas/sections, both fed by a `FileSystemWatcher` over `<workspaceRoot>/qdk-learning.json`. This panel is independent of the MCP/web/TUI bundle above — it runs in the extension host and only reads the progress file written by `KatasServer`.

- **`detector.ts`** — single source of truth for "is there a katas workspace, and where is it?". Scans each open `vscode.workspace.workspaceFolders` for an existing `qdk-learning.json`. Returns `{ workspaceRoot, katasRoot, learningFile, katasDirExists }`. **Also used by [`src/katasMcp.ts`](../../source/vscode/src/katasMcp.ts)** to launch the MCP server with `--workspace` and skip the chat-side `init` round-trip.
- **`progressReader.ts`** — `ProgressWatcher` runs the detector, watches `qdk-learning.json`, parses it against the catalog, exposes `lastSnapshot` + `onDidChange`, and maintains the `qsharp-vscode.katasDetected` context key (drives view welcome content).
- **`catalog.ts`** — pulls the kata list dynamically from `qsharp-lang/katas-md` (`getAllKatas()`) so the panel never hardcodes content. Cached. `RECOMMENDED_ORDER` controls display order.
- **`treeProvider.ts`** — kata → section nodes. **No `item.command`** — clicking a node is a no-op by design. Each tree item exposes a `contextValue` (`kata` | `lessonSection` | `exerciseSection`) for the inline chat-bubble action.
- **`overviewProvider.ts`** — a `WebviewViewProvider` with two states: a **landing page** (welcome + Get started) when no katas workspace is detected, and a **tracker** (animated progress ring, "up next" card, contextual encouragement, Continue button) once one is. CSP+nonce, no external assets, all messages go through `postMessage` (`ready` / `continue` / `setup`).
- **`commands.ts`** — registers `qsharp-vscode.katasRefresh`, `katasContinue`, `katasOpenSection`, `katasAskInChat`. Exercise actions open the scaffolded `.qs` file directly; lesson actions and `katasAskInChat` open the chat view (`workbench.action.chat.open`) with a pre-built prompt that activates the `QDK Learning` agent.
- **`index.ts`** — `registerKatasProgressView(context)` wires the watcher → tree + webview + commands. Called from `extension.ts` after `registerKatasMcpServer(context)`.

When changing the panel:

- The detector contract is shared with `katasMcp.ts`. Adding fields is fine; renaming or removing requires updating both call sites and the eager-init path in `learning/index.ts`.
- The webview HTML lives inline in `overviewProvider.getHtml()` — keep it small, themeable via `--vscode-*` CSS variables, and CSP-clean (no inline event handlers).
- Telemetry uses `EventType.KatasPanelAction` with the `action` property (see `telemetry.ts`).

## External references

Fetch canonical sources for non-trivial MCP-design decisions:

- MCP Apps spec: https://raw.githubusercontent.com/modelcontextprotocol/ext-apps/refs/heads/main/specification/2026-01-26/apps.mdx
- MCP Apps patterns: https://raw.githubusercontent.com/modelcontextprotocol/ext-apps/refs/heads/main/docs/patterns.md
- MCP base spec: https://modelcontextprotocol.io/specification/2025-11-25.md
