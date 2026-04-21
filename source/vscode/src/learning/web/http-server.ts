// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";
import { existsSync, readFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join, extname } from "node:path";
import { fileURLToPath } from "node:url";
import type { KatasServer } from "../server/index.js";
import type {
  OverallProgress,
  ServerState,
  StatefulResult,
} from "../server/index.js";

const __dirname = fileURLToPath(new URL(".", import.meta.url));

// Runtime paths differ between dev (tsx from src/) and bundled (single file at
// out/learning/index.js). In the bundle, web assets live at
// `<__dirname>/web/public` and the widget template at `<__dirname>/widget/app.html`.
// In dev (src/learning/web/http-server.ts), __dirname already IS .../web.
const STATIC_DIR = existsSync(join(__dirname, "web", "public"))
  ? join(__dirname, "web", "public")
  : join(__dirname, "public");
const WIDGET_TEMPLATE_PATH = existsSync(join(__dirname, "widget", "app.html"))
  ? join(__dirname, "widget", "app.html")
  : join(__dirname, "..", "mcp", "widget", "app.html");
const WIDGET_CSS_PATH = existsSync(join(__dirname, "widget", "app.css"))
  ? join(__dirname, "widget", "app.css")
  : join(__dirname, "..", "mcp", "widget", "app.css");

const MIME_TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
  ".ttf": "font/ttf",
};

/** Convert OverallProgress.katas Map to a plain object for JSON serialization. */
function serializeProgress(progress: OverallProgress): object {
  return {
    ...progress,
    katas: Object.fromEntries(progress.katas),
  };
}

/** Serialize a full ServerState (progress contains a Map). */
function serializeState(state: ServerState): object {
  return { ...state, progress: serializeProgress(state.progress) };
}

/** Serialize a `{ result, state }` envelope. */
function serializeStateful<T>(r: StatefulResult<T>): object {
  return { result: r.result, state: serializeState(r.state) };
}

function json(res: ServerResponse, data: unknown, status = 200): void {
  res.writeHead(status, { "Content-Type": "application/json; charset=utf-8" });
  res.end(JSON.stringify(data));
}

function error(res: ServerResponse, status: number, message: string): void {
  json(res, { error: message }, status);
}

async function readBody(
  req: IncomingMessage,
): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) {
    chunks.push(chunk as Buffer);
  }
  const text = Buffer.concat(chunks).toString("utf-8");
  if (!text) return {};
  return JSON.parse(text) as Record<string, unknown>;
}

export function createHttpServer(server: KatasServer) {
  // Resolve static file directory — in dev (tsx) it's src/web/public,
  // in compiled mode it's out/learning/web/public. We look relative to this file.
  const staticDir = STATIC_DIR;

  const require0 = createRequire(import.meta.url);
  const katexDir = join(
    dirname(require0.resolve("katex/package.json")),
    "dist",
  );

  // Build the MCP widget HTML once, with an HTTP-bridge ExtApps shim instead
  // of the real ext-apps bundle. This lets you preview the MCP widget at
  // /widget without needing to load it inside VS Code chat.
  let widgetHtml: string | null = null;
  const getWidgetHtml = (): string => {
    if (widgetHtml === null) widgetHtml = buildWidgetTestHtml();
    return widgetHtml;
  };

  return createServer(async (req, res) => {
    const url = new URL(req.url ?? "/", `http://${req.headers.host}`);
    const method = req.method ?? "GET";
    const path = url.pathname;

    // CORS headers for local development
    res.setHeader("Access-Control-Allow-Origin", "*");
    res.setHeader("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
    res.setHeader("Access-Control-Allow-Headers", "Content-Type");
    if (method === "OPTIONS") {
      res.writeHead(204);
      res.end();
      return;
    }

    try {
      // ─── API routes ───
      if (path.startsWith("/api/")) {
        await handleApi(server, method, path, req, res);
        return;
      }

      // ─── KaTeX static files ───
      if (path.startsWith("/katex/")) {
        const katexFile = path.slice("/katex/".length);
        const resolved = join(katexDir, katexFile);
        if (!resolved.startsWith(katexDir)) {
          error(res, 403, "Forbidden");
          return;
        }
        try {
          const content = await readFile(resolved);
          const ext = extname(resolved);
          res.writeHead(200, {
            "Content-Type": MIME_TYPES[ext] ?? "application/octet-stream",
          });
          res.end(content);
        } catch {
          error(res, 404, "Not found");
        }
        return;
      }

      // ─── MCP widget preview (real widget HTML + HTTP-bridge ExtApps shim) ───
      if (path === "/widget" || path === "/widget/") {
        try {
          const html = getWidgetHtml();
          res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
          res.end(html);
        } catch (err) {
          error(res, 500, err instanceof Error ? err.message : String(err));
        }
        return;
      }

      // ─── Static files ───
      const filePath = path === "/" ? "/index.html" : path;
      // Prevent path traversal
      const resolved = join(staticDir, filePath);
      if (!resolved.startsWith(staticDir)) {
        error(res, 403, "Forbidden");
        return;
      }

      try {
        const content = await readFile(resolved);
        const ext = extname(resolved);
        res.writeHead(200, {
          "Content-Type": MIME_TYPES[ext] ?? "application/octet-stream",
        });
        res.end(content);
      } catch {
        error(res, 404, "Not found");
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      error(res, 500, message);
    }
  });
}

async function handleApi(
  server: KatasServer,
  method: string,
  path: string,
  req: IncomingMessage,
  res: ServerResponse,
): Promise<void> {
  // GET endpoints
  if (method === "GET") {
    switch (path) {
      case "/api/state":
        json(res, serializeState(server.getState()));
        return;

      case "/api/katas":
        json(res, server.listKatas());
        return;

      case "/api/solution":
        json(res, { code: server.getFullSolution() });
        return;

      case "/api/progress":
        json(res, serializeProgress(server.getProgress()));
        return;

      case "/api/code":
        json(res, { code: await server.readUserCode() });
        return;
    }
  }

  // POST endpoints
  if (method === "POST") {
    const body = await readBody(req);

    switch (path) {
      case "/api/next": {
        const r = server.next();
        json(res, { moved: r.moved, state: serializeState(r.state) });
        return;
      }

      case "/api/previous": {
        const r = server.previous();
        json(res, { moved: r.moved, state: serializeState(r.state) });
        return;
      }

      case "/api/goto": {
        const state = server.goTo(
          body.kataId as string,
          (body.sectionIndex as number) ?? 0,
          (body.itemIndex as number) ?? 0,
        );
        json(res, serializeState(state));
        return;
      }

      case "/api/run":
        json(
          res,
          serializeStateful(await server.run(body.shots as number | undefined)),
        );
        return;

      case "/api/run-noise":
        json(
          res,
          serializeStateful(
            await server.runWithNoise(body.shots as number | undefined),
          ),
        );
        return;

      case "/api/circuit":
        json(res, serializeStateful(await server.getCircuit()));
        return;

      case "/api/estimate":
        json(res, serializeStateful(await server.getResourceEstimate()));
        return;

      case "/api/check":
        json(res, serializeStateful(await server.checkSolution()));
        return;

      case "/api/hint":
        json(res, serializeStateful(server.getNextHint()));
        return;

      case "/api/reveal-answer":
        json(res, serializeStateful(server.revealAnswer()));
        return;

      case "/api/ai/hint":
        json(res, serializeStateful(await server.getAIHint()));
        return;

      case "/api/ai/ask":
        json(
          res,
          serializeStateful(
            await server.askConceptQuestion(body.question as string),
          ),
        );
        return;

      case "/api/ai/review":
        json(res, serializeStateful(await server.reviewSolution()));
        return;
    }
  }

  error(res, 404, `Unknown endpoint: ${method} ${path}`);
}

// ─── MCP widget preview ───────────────────────────────────────────────────
//
// The MCP widget HTML at src/mcp/widget/app.html is built around a small host
// bridge (`globalThis.ExtApps.App`) that exposes `callServerTool`,
// `sendMessage`, `openLink`, and an `ontoolresult` callback. In production the
// MCP server inlines `@modelcontextprotocol/ext-apps` to provide that bridge.
//
// For local browser previewing we substitute a tiny HTTP shim that maps each
// `callServerTool({name, arguments})` to the matching `/api/*` endpoint on
// this same web server, and stubs `sendMessage`/`openLink` with `console.log`
// so the widget runs end-to-end without needing the MCP host.
function buildWidgetTestHtml(): string {
  const require = createRequire(import.meta.url);

  // Same shared rendering bundle the MCP widget and the standalone web app use.
  const sharedDir = join(STATIC_DIR, "shared");
  const renderJs = readFileSync(join(sharedDir, "render.js"), "utf8");

  // Inline KaTeX so math renders without external requests (matches MCP widget).
  const katexDir = dirname(require.resolve("katex/package.json"));
  const katexJs = readFileSync(join(katexDir, "dist", "katex.min.js"), "utf8");
  const katexAutoRenderJs = readFileSync(
    join(katexDir, "dist", "contrib", "auto-render.min.js"),
    "utf8",
  );

  // Read the actual MCP widget template — the real thing the user sees in chat.
  const template = readFileSync(WIDGET_TEMPLATE_PATH, "utf8");
  const widgetCss = readFileSync(WIDGET_CSS_PATH, "utf8");

  return template
    .replace("/*__EXT_APPS_BUNDLE__*/", () => EXT_APPS_HTTP_SHIM)
    .replace("/*__KATAS_RENDER__*/", () => renderJs)
    .replace("/*__KATEX_JS__*/", () => katexJs)
    .replace("/*__KATEX_AUTORENDER_JS__*/", () => katexAutoRenderJs)
    .replace("/*__KATAS_WIDGET_CSS__*/", () => widgetCss);
}

/**
 * Browser-side stub of `globalThis.ExtApps` that proxies tool calls to the
 * web server's `/api/*` endpoints. Mirrors just enough of the
 * `@modelcontextprotocol/ext-apps` App surface for the widget to run.
 */
const EXT_APPS_HTTP_SHIM = String.raw`
(() => {
  // Map each MCP tool name → which fetch we should issue, and how to translate
  // the JSON response back into the envelope shape the widget expects.
  const ROUTES = {
    get_state:      { method: "GET",  path: "/api/state",         wrap: (s)        => ({ state: s }) },
    next:           { method: "POST", path: "/api/next",          wrap: (e)        => e },
    previous:       { method: "POST", path: "/api/previous",      wrap: (e)        => e },
    run:            { method: "POST", path: "/api/run",           wrap: (e)        => e },
    run_with_noise: { method: "POST", path: "/api/run-noise",     wrap: (e)        => e },
    circuit:        { method: "POST", path: "/api/circuit",       wrap: (e)        => e },
    estimate:       { method: "POST", path: "/api/estimate",      wrap: (e)        => e },
    check:          { method: "POST", path: "/api/check",         wrap: (e)        => e },
    hint:           { method: "POST", path: "/api/hint",          wrap: (e)        => e },
    reveal_answer:  { method: "POST", path: "/api/reveal-answer", wrap: (e)        => e },
    ai_hint:        { method: "POST", path: "/api/ai/hint",       wrap: (e)        => e },
    ask_ai:         { method: "POST", path: "/api/ai/ask",        wrap: (e)        => e },
    solution:       { method: "GET",  path: "/api/solution",      wrap: ({code}, s) => ({ result: code, state: s }) },
    list_katas:     { method: "GET",  path: "/api/katas",         wrap: (katas, s) => ({ result: katas, state: s }) },
    get_progress:   { method: "GET",  path: "/api/progress",      wrap: (progress, s) => ({ progress, state: s }) },
    goto:           { method: "POST", path: "/api/goto",          wrap: (state)    => ({ state }) },
  };

  class App extends EventTarget {
    constructor(_info, _opts) { super(); this.ontoolresult = null; this.onteardown = null; }
    async connect() { /* no-op */ }
    getHostVersion() { return { name: "browser-preview", version: "0.0.0" }; }
    getHostContext() { return null; }
    async requestTeardown() { /* no-op in preview */ }

    async callServerTool({ name, arguments: args }) {
      const route = ROUTES[name];
      if (!route) {
        return { isError: true, content: [{ type: "text", text: "Unknown tool: " + name }] };
      }
      const init = { method: route.method, headers: {} };
      if (route.method === "POST") {
        init.headers["Content-Type"] = "application/json";
        init.body = JSON.stringify(args || {});
      }
      let res;
      try {
        res = await fetch(route.path, init);
      } catch (err) {
        return { isError: true, content: [{ type: "text", text: "fetch failed: " + (err && err.message || err) }] };
      }
      const json = await res.json().catch(() => null);
      if (!res.ok) {
        const msg = (json && json.error) || ("HTTP " + res.status);
        return { isError: true, content: [{ type: "text", text: msg }] };
      }
      // For tools whose endpoint doesn't include the latest state, fetch it
      // separately so the widget can refresh its position/actions/progress.
      let state;
      if (route.wrap.length >= 2) {
        const sRes = await fetch("/api/state");
        state = await sRes.json();
      }
      const envelope = route.wrap(json, state);
      const text = JSON.stringify(envelope);
      // Fire ontoolresult so the widget's host-driven sync path runs too,
      // matching real MCP behavior where the host echoes results to the iframe.
      const content = [{ type: "text", text }];
      if (typeof this.ontoolresult === "function") {
        try { this.ontoolresult({ content }); } catch (err) { console.error(err); }
      }
      return { isError: false, content };
    }

    async sendMessage(msg) {
      console.log("[widget-test] sendMessage (would go to chat):", msg);
      // Surface a hint in the page so the test user sees something happened.
      const banner = document.createElement("div");
      banner.style.cssText = "position:fixed;bottom:8px;left:50%;transform:translateX(-50%);background:#333;color:#fff;padding:6px 12px;border-radius:4px;font-size:11px;z-index:9999;opacity:0.9;";
      const text = (msg && msg.content && msg.content[0] && msg.content[0].text) || "(message)";
      banner.textContent = "sendMessage → chat: " + text.slice(0, 80);
      document.body.appendChild(banner);
      setTimeout(() => banner.remove(), 2500);
    }

    async openLink({ url }) {
      console.log("[widget-test] openLink:", url);
      // In the browser preview, just open the link in a new tab.
      window.open(url, "_blank", "noopener");
    }
  }

  globalThis.ExtApps = { App };
})();
`;
