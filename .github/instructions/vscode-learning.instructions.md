---
applyTo: "source/vscode/src/learning/**,source/vscode/src/katasMcp.ts,source/vscode/skills/quantum-katas/**"
description: "Q# Quantum Katas MCP server, TUI, and web UI embedded in the VS Code extension."
---

# source/vscode/src/learning

This folder builds a single Node bundle (`out/learning/index.js`) that provides four front-ends over one shared `KatasServer`:

- **MCP stdio** — `node out/learning/index.js --mcp`. Launched by the extension via `vscode.lm.registerMcpServerDefinitionProvider` (see [`src/katasMcp.ts`](../../source/vscode/src/katasMcp.ts)).
- **MCP HTTP** — `node out/learning/index.js --mcp-http --port <P>` (Streamable HTTP at `/mcp`). Used for smoke tests.
- **Web UI** — `node out/learning/index.js --web --port <P>` serves the standalone app at `/` and an MCP-widget preview at `/widget`.
- **TUI** — `node out/learning/index.js` (interactive terminal; requires a real TTY).

Use `--workspace <path>` to control where exercise `.qs` files and `.katas-progress.json` are scaffolded. Without it, a `quantum-katas/` folder is created in the CWD (gitignored repo-wide).

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

## Keep the skill in sync

[`source/vscode/skills/quantum-katas/SKILL.md`](../../source/vscode/skills/quantum-katas/SKILL.md) documents the MCP tools for agents. When you change anything user-visible in `mcp/server.ts` or `mcp/widget/app.html`, update SKILL.md in the same change.

## External references

Fetch canonical sources for non-trivial MCP-design decisions:

- MCP Apps spec: https://raw.githubusercontent.com/modelcontextprotocol/ext-apps/refs/heads/main/specification/2026-01-26/apps.mdx
- MCP Apps patterns: https://raw.githubusercontent.com/modelcontextprotocol/ext-apps/refs/heads/main/docs/patterns.md
- MCP base spec: https://modelcontextprotocol.io/specification/2025-11-25.md
