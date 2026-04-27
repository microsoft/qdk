// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import {
  createServer as createHttpServerNode,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";
import { randomUUID } from "node:crypto";
import {
  registerAppTool,
  registerAppResource,
  RESOURCE_MIME_TYPE,
} from "@modelcontextprotocol/ext-apps/server";
import { z } from "zod";
import {
  readFileSync,
  existsSync,
  statSync,
  watch,
  readFile,
  unlink,
  writeFileSync,
  mkdirSync,
} from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve, isAbsolute } from "node:path";

const NAVIGATE_FILE = ".navigate.json";
const OPEN_PANEL_FILE = ".open-panel";
const LEARNING_FILE = "qdk-learning.json";
import { createRequire } from "node:module";
import type {
  KatasServer,
  IAIProvider,
  OverallProgress,
  ServerState,
} from "../server/index.js";

const WIDGET_URI = "ui://katas/app.html";

/**
 * Serialize OverallProgress for MCP responses. The full per-kata/per-section
 * breakdown can be huge (every section title etc.); the agent rarely needs it,
 * so by default we ship just the headline stats + current position, plus the
 * breakdown for the *current* kata only (so the widget can render its
 * progress segments). Use `serializeProgressFull` (or call `get_progress`)
 * when the full breakdown is needed.
 */
function serializeProgress(progress: OverallProgress): object {
  const cur = progress.currentPosition?.kataId;
  const currentKata = cur ? progress.katas.get(cur) : undefined;
  return {
    stats: progress.stats,
    currentPosition: progress.currentPosition,
    katas: currentKata ? { [cur as string]: currentKata } : {},
  };
}

function serializeProgressFull(progress: OverallProgress): object {
  return { ...progress, katas: Object.fromEntries(progress.katas) };
}

function serializeState(state: ServerState): object {
  return { ...state, progress: serializeProgress(state.progress) };
}

/** Wrap a method result as an MCP tool response with embedded state. */
function wrapResult(value: unknown): {
  content: [{ type: "text"; text: string }];
} {
  const text = typeof value === "string" ? value : JSON.stringify(value);
  return { content: [{ type: "text", text }] };
}

/** Return a structured error response for an MCP tool call. */
function errorResponse(message: string): {
  isError: true;
  content: [{ type: "text"; text: string }];
} {
  return {
    isError: true,
    content: [{ type: "text", text: message }],
  };
}

const NOT_INITIALIZED_MESSAGE =
  "The katas workspace has not been initialized. The agent must call `init` with an absolute path before any other tool can be used. " +
  "Look for an existing `qdk-learning.json` file in the user's current VS Code workspace and pass that workspace root as `workspacePath`. " +
  "If none exists, ask the user where they'd like to store exercise files and progress.";

function buildWidgetHtml(): string {
  const require = createRequire(import.meta.url);
  const __dirname = dirname(fileURLToPath(import.meta.url));

  // Inline the ext-apps browser bundle. The iframe CSP blocks CDN fetches.
  const bundlePath =
    require.resolve("@modelcontextprotocol/ext-apps/app-with-deps");
  const rawBundle = readFileSync(bundlePath, "utf8");
  const extAppsBundle = rawBundle.replace(
    /export\s*\{([^}]+)\};?\s*$/,
    (_, body: string) =>
      "globalThis.ExtApps={" +
      body
        .split(",")
        .map((p) => {
          const [local, exported] = p.split(" as ").map((s) => s.trim());
          return `${exported ?? local}:${local}`;
        })
        .join(",") +
      "};",
  );

  // Inline the shared rendering modules (same files the web UI uses).
  // In the bundle, __dirname is `out/learning/`, with assets laid out at
  // `web/public/shared/*` and `widget/app.html` beside `index.js`.
  // In dev (tsx), __dirname is `src/learning/mcp/`; try bundle layout first
  // then fall back to the dev layout.
  const sharedDir = existsSync(join(__dirname, "web", "public", "shared"))
    ? join(__dirname, "web", "public", "shared")
    : join(__dirname, "..", "web", "public", "shared");
  const renderJs = readFileSync(join(sharedDir, "render.js"), "utf8");
  const uiJs = readFileSync(join(sharedDir, "ui.js"), "utf8");

  // Inline KaTeX (core + auto-render) so math renders inside the iframe
  // without external network or font loads. We configure MathML-only output
  // so the browser handles glyph layout natively (no KaTeX CSS/fonts needed).
  const katexDir = dirname(require.resolve("katex/package.json"));
  const katexJs = readFileSync(join(katexDir, "dist", "katex.min.js"), "utf8");
  const katexAutoRenderJs = readFileSync(
    join(katexDir, "dist", "contrib", "auto-render.min.js"),
    "utf8",
  );

  const widgetDir = join(__dirname, "widget");
  const template = readFileSync(join(widgetDir, "app.html"), "utf8");
  const widgetCss = readFileSync(join(widgetDir, "app.css"), "utf8");
  return template
    .replace("/*__EXT_APPS_BUNDLE__*/", () => extAppsBundle)
    .replace("/*__KATAS_RENDER__*/", () => renderJs)
    .replace("/*__KATAS_UI__*/", () => uiJs)
    .replace("/*__KATEX_JS__*/", () => katexJs)
    .replace("/*__KATEX_AUTORENDER_JS__*/", () => katexAutoRenderJs)
    .replace("/*__KATAS_WIDGET_CSS__*/", () => widgetCss);
}

/**
 * Register all katas tools + the widget resource on the given MCP server.
 * Does NOT connect any transport — callers pick stdio (`runMCPServerStdio`)
 * or HTTP (`runMCPServerHttp`). Keeping registration in one place ensures the
 * two deployments never drift in behavior.
 *
 * The KatasServer is NOT initialized here unless `initOptions.initialWorkspace`
 * is set. When the host knows the workspace path up front (e.g. the VS Code
 * extension auto-discovered a `qdk-learning.json` file), it can pre-call
 * `KatasServer.initialize` and pass that path here — the per-session state
 * will start with `initialized = true` and the agent does not need to call
 * `init`. Otherwise the agent must call `init` first — that
 * tool elicits user confirmation and then initializes the server.
 */
export async function registerMCPHandlers(
  server: KatasServer,
  mcp: McpServer,
  initOptions: {
    aiProvider: IAIProvider;
    contentFormat: "html" | "markdown";
    /**
     * If set, treat the workspace as already initialized at this absolute path.
     * Callers must have already invoked `KatasServer.initialize` with the same
     * path before connecting the transport.
     */
    initialWorkspace?: string;
    /** Absolute path to the katas content folder (resolved from qdk-learning.json). */
    initialKatasRoot?: string;
  },
): Promise<void> {
  const widgetHtml = buildWidgetHtml();
  const uiMeta = { ui: { resourceUri: WIDGET_URI } };

  // Closure state: tracks whether the agent has set a workspace yet.
  let initialized = initOptions.initialWorkspace != null;
  let currentWorkspacePath: string | null =
    initOptions.initialWorkspace ?? null;

  // ─── Widget identity tracking ─────────────────────────────────────────
  //
  // Per the MCP Apps spec, every `_meta.ui` tool call mounts a fresh
  // widget instance. Older widgets remain visible in chat scrollback and
  // stay interactive. We mint a fresh widgetId on each widget-mounting
  // call (`render_state`, `goto`) and remember it as
  // `liveWidgetId`. Widget-initiated tool calls pass `widgetId`; if it
  // doesn't match `liveWidgetId`, the call returns `{ stale: true }`
  // with current server state and WITHOUT executing — the user sees an
  // amber "view replaced" banner and current data, and must click on
  // the live widget to actually act.
  let widgetSeq = 0;
  let liveWidgetId: string | null = null;

  function mintWidgetId(): string {
    widgetSeq += 1;
    const id = `w${widgetSeq}`;
    liveWidgetId = id;
    return id;
  }

  /** Returns a stale envelope (with current state) if widgetId is set and
   *  doesn't match live, else null (caller proceeds normally). */
  function checkStale(widgetId: string | undefined) {
    if (widgetId && widgetId !== liveWidgetId) {
      return wrapResult({
        stale: true,
        liveWidgetId,
        state: serializeState(server.getState()),
      });
    }
    return null;
  }

  /**
   * Wrap a tool handler so it returns a structured error when the workspace
   * hasn't been set. The agent reads `isError: true` and the message to know
   * it must call `init` first.
   */
  const requireInit = <A, R>(
    handler: (args: A) => Promise<R> | R,
  ): ((args: A) => Promise<R | ReturnType<typeof errorResponse>>) => {
    return async (args: A) => {
      if (!initialized) {
        return errorResponse(NOT_INITIALIZED_MESSAGE);
      }
      return handler(args);
    };
  };

  // ─── External navigation (.navigate.json) ────────────────────────────
  //
  // The VS Code tree view can write a `.navigate.json` file into the
  // katas workspace to signal a navigation request without going through
  // chat. We `fs.watch` that file; when it appears the server reads it,
  // deletes it, and calls `goTo()`. The result is stashed in
  // `pendingNavigation` until the live widget picks it up via the
  // app-only `check_navigate` tool (polled at ~500ms by visible widgets).
  let pendingNavigation: object | null = null;
  let navigateWatcher: ReturnType<typeof watch> | null = null;

  function startNavigateWatcher(katasRoot: string): void {
    stopNavigateWatcher();
    const navPath = join(katasRoot, NAVIGATE_FILE);
    try {
      navigateWatcher = watch(katasRoot, (eventType, filename) => {
        // On Windows, filename should always be provided but may differ
        // in casing. On some platforms it can be null — fall back to
        // trying to read the file unconditionally in that case.
        if (
          filename != null &&
          filename.toLowerCase() !== NAVIGATE_FILE.toLowerCase()
        )
          return;
        // Read and delete in one go — fire-and-forget.
        readFile(navPath, "utf-8", (readErr, data) => {
          if (readErr) return; // File may already be gone (race).
          unlink(navPath, () => {
            // Ignore unlink errors — file may already be deleted.
          });
          try {
            const req = JSON.parse(data) as {
              kataId?: string;
              sectionId?: string;
              itemIndex?: number;
            };
            if (req.kataId) {
              const state = server.goTo(
                req.kataId,
                req.sectionId,
                req.itemIndex ?? 0,
              );
              pendingNavigation = serializeState(state);
              process.stderr.write(
                `[katas-mcp] navigate signal consumed: ${req.kataId}§${req.sectionId ?? ""}\n`,
              );
            }
          } catch {
            // Malformed JSON — ignore.
          }
        });
      });
      process.stderr.write(
        `[katas-mcp] navigate watcher started on ${katasRoot}\n`,
      );
    } catch (err) {
      process.stderr.write(`[katas-mcp] navigate watcher failed: ${err}\n`);
    }
  }

  function stopNavigateWatcher(): void {
    if (navigateWatcher) {
      navigateWatcher.close();
      navigateWatcher = null;
    }
  }

  // Closure state: tracks the resolved katas root for navigate watcher.
  let currentKatasRoot: string | null = initOptions.initialKatasRoot ?? null;

  // If workspace was pre-discovered, start watching immediately.
  if (currentKatasRoot) {
    startNavigateWatcher(currentKatasRoot);
  }

  // ─── Widget resources ───
  registerAppResource(
    mcp,
    "Q# Katas Widget",
    WIDGET_URI,
    { description: "Interactive Q# katas UI" },
    async () => ({
      contents: [
        { uri: WIDGET_URI, mimeType: RESOURCE_MIME_TYPE, text: widgetHtml },
      ],
    }),
  );

  // ─── Workspace lifecycle ───

  mcp.registerTool(
    "get_workspace",
    {
      description:
        "Report the currently-configured katas workspace path, or null if `init` has not been called yet. Safe to call at any time. Does not open the widget.",
    },
    async () =>
      wrapResult({
        workspacePath: currentWorkspacePath,
        initialized,
      }),
  );

  mcp.registerTool(
    "init",
    {
      description:
        "Initialize (or reinitialize) the katas workspace. Creates a `qdk-learning.json` file in the given directory and scaffolds exercise files. " +
        "The agent is responsible for choosing a sensible path: prefer the user's current VS Code workspace root. " +
        "If `qdk-learning.json` already exists at the given path, the workspace is adopted immediately. Otherwise the user is prompted to confirm via an elicitation prompt; if they decline or cancel, the workspace is left unset and the call returns an error. " +
        "This tool does not open the katas widget; the agent should call `render_state` afterward to render the tutor.",
      inputSchema: {
        workspacePath: z
          .string()
          .min(1)
          .describe(
            "Absolute path to the workspace directory where `qdk-learning.json` will be created. Must be absolute; relative paths are rejected.",
          ),
        katasRoot: z
          .string()
          .optional()
          .describe(
            'Relative path from workspacePath to the katas content folder (exercises, examples). Defaults to "./quantum-katas".',
          ),
        kataIds: z
          .array(z.string())
          .optional()
          .describe(
            "Optional list of kata IDs to load. If omitted, all katas are loaded.",
          ),
      },
    },
    async ({ workspacePath, katasRoot: katasRootArg, kataIds }) => {
      if (!isAbsolute(workspacePath)) {
        return errorResponse(
          `workspacePath must be absolute; got "${workspacePath}". Resolve it on the agent side (e.g. using the VS Code workspace root) before calling init.`,
        );
      }
      const resolved = resolve(workspacePath);
      const katasRootRel = katasRootArg ?? "./quantum-katas";
      const learningFilePath = join(resolved, LEARNING_FILE);
      const resolvedKatasRoot = resolve(resolved, katasRootRel);

      // Gather some pre-confirmation facts for the user so they know what will happen.
      let exists = false;
      let isDir = false;
      let hasExistingLearningFile = false;
      try {
        if (existsSync(resolved)) {
          exists = true;
          isDir = statSync(resolved).isDirectory();
          hasExistingLearningFile = existsSync(learningFilePath);
        }
      } catch {
        // Fall through — elicitation will still ask.
      }
      if (exists && !isDir) {
        return errorResponse(`Path exists but is not a directory: ${resolved}`);
      }

      const summary = hasExistingLearningFile
        ? `Use existing katas workspace at ${resolved}.`
        : exists
          ? `Initialize a katas workspace at ${resolved}.`
          : `Create ${resolved} and initialize a katas workspace.`;

      // If a valid qdk-learning.json already exists at the target path,
      // skip the user-confirmation elicitation — the directory was clearly set
      // up for this purpose previously, so prompting again is just noise.
      if (!hasExistingLearningFile) {
        let elicitResult: Awaited<ReturnType<typeof mcp.server.elicitInput>>;
        try {
          elicitResult = await mcp.server.elicitInput({
            message: `The Q# Katas MCP server wants to use this workspace:\n\n  ${resolved}\n\n${summary}\n\nExercise files will be read/written here and progress will be saved in \`qdk-learning.json\`.`,
            requestedSchema: {
              type: "object",
              properties: {
                confirm: {
                  type: "boolean",
                  title: "Use this workspace",
                  description:
                    "Confirm to proceed with the workspace path above.",
                  default: true,
                },
              },
            },
          });
        } catch (err: unknown) {
          const msg = err instanceof Error ? err.message : String(err);
          return errorResponse(
            `Could not prompt the user to confirm the workspace path (elicitation not supported by the host?): ${msg}. ` +
              `Refusing to initialize without explicit user confirmation.`,
          );
        }

        if (elicitResult.action !== "accept") {
          return errorResponse(
            `User ${elicitResult.action === "decline" ? "declined" : "cancelled"} the workspace confirmation. Workspace remains unset; ask the user where they'd like to store kata files and try again.`,
          );
        }
        const confirm = (
          elicitResult.content as Record<string, unknown> | undefined
        )?.confirm;
        if (confirm === false) {
          return errorResponse(
            `User did not confirm the workspace path. Workspace remains unset.`,
          );
        }

        // Create qdk-learning.json with default content.
        try {
          mkdirSync(resolved, { recursive: true });
          const defaultData = {
            version: 1,
            katasRoot: katasRootRel,
            position: { kataId: "", sectionId: "", itemIndex: 0 },
            completions: {},
            startedAt: new Date().toISOString(),
          };
          writeFileSync(
            learningFilePath,
            JSON.stringify(defaultData, null, 2),
            "utf-8",
          );
        } catch (err: unknown) {
          const msg = err instanceof Error ? err.message : String(err);
          return errorResponse(
            `Failed to create ${LEARNING_FILE} at ${resolved}: ${msg}`,
          );
        }
      }

      try {
        await server.initialize({
          kataIds: kataIds ?? [],
          learningFilePath,
          katasRoot: resolvedKatasRoot,
          katasRootRel,
          aiProvider: initOptions.aiProvider,
          contentFormat: initOptions.contentFormat,
        });
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        return errorResponse(
          `Failed to initialize workspace at ${resolved}: ${msg}`,
        );
      }

      initialized = true;
      currentWorkspacePath = resolved;
      currentKatasRoot = resolvedKatasRoot;
      startNavigateWatcher(resolvedKatasRoot);
      return wrapResult({
        workspacePath: resolved,
        katasRoot: resolvedKatasRoot,
        state: serializeState(server.getState()),
      });
    },
  );

  // ─── Debugging ───

  // Reports the MCP server process's view of its own environment. Useful for
  // diagnosing host-launch issues (working directory, argv, env).
  mcp.registerTool(
    "cwd",
    {
      description:
        "Debug: report the MCP server process's working directory, argv, Node version, pid, exec path, module dir, and current workspace state.",
    },
    async () => {
      return wrapResult({
        cwd: process.cwd(),
        pid: process.pid,
        node: process.version,
        platform: process.platform,
        argv: process.argv,
        execPath: process.execPath,
        moduleDir: dirname(fileURLToPath(import.meta.url)),
        currentWorkspacePath,
        initialized,
      });
    },
  );

  // ─── Read tools (no state change) ───
  //
  // `render_state` mounts the widget and returns a fresh `widgetId` for it
  // to use on follow-up calls. Use this when the user wants to start or
  // resume an interactive katas session.
  //
  // `get_state` is a plain (non-widget) read used by the agent to check
  // current server state without re-rendering — e.g. after the user has
  // been clicking around in the widget and the agent wants to catch up.

  registerAppTool(
    mcp,
    "render_state",
    {
      description:
        "Open the interactive Q# Katas widget at the current position. The widget renders the current lesson/example/question/exercise itself, plus a clickable action bar that invokes katas tools directly without sending messages back to the chat. The agent should NOT re-render `state.position.item` in chat \u2014 the widget owns that. Call this once to start or resume a session, then rely on the widget for further interaction.",
      _meta: uiMeta,
    },
    requireInit(async () =>
      wrapResult({
        widgetId: mintWidgetId(),
        state: serializeState(server.getState()),
      }),
    ),
  );

  mcp.registerTool(
    "get_state",
    {
      description:
        "Read the current katas position and progress without mounting or refreshing any widget. Use this when an active widget exists and the user has likely interacted with it (so server state may have moved on without the agent's knowledge), and the agent needs to catch up before answering a question. Does NOT consume an LLM turn for the user. Plain (non-widget) tool.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      return wrapResult({ state: serializeState(server.getState()) });
    }),
  );

  // Full per-kata progress breakdown. Separate from get_state so the default
  // tool responses stay small — most chat turns don't need it.
  mcp.registerTool(
    "get_progress",
    {
      description:
        "Return the full per-kata progress breakdown. Plain (non-widget) tool \u2014 the agent renders the breakdown in chat.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      return wrapResult({
        progress: serializeProgressFull(server.getProgress()),
        state: serializeState(server.getState()),
      });
    }),
  );

  // `list_katas` is a plain (non-widget) tool. The agent calls it from
  // chat and renders the catalog there (e.g. as a numbered list); it
  // then follows up with `goto` to jump to a chosen kata. The widget
  // never invokes this tool, so it has no `widgetId` / stale-check.
  mcp.registerTool(
    "list_katas",
    {
      description:
        "List all available katas with their completion status. Plain (non-widget) tool \u2014 the agent renders the catalog in chat (e.g. as a numbered list) and then calls `goto` with the chosen `kataId` to jump. Does NOT mount or refresh the widget. Requires `init` to have been called first.",
    },
    requireInit(async () =>
      wrapResult({
        result: server.listKatas(),
        state: serializeState(server.getState()),
      }),
    ),
  );

  // ─── Navigation ───
  // All navigation tools open the widget. The widget owns rendering — the
  // agent should NOT re-render `state.position.item` in chat after a nav
  // tool call. Subsequent navigation typically happens via widget buttons,
  // which call these tools directly and don't consume LLM turns.

  mcp.registerTool(
    "next",
    {
      description:
        "Move to the next item (lesson text, example, question, or exercise). Plain tool \u2014 does not mount a widget. The widget calls this via `tools/call`; if the agent calls it directly, it should follow up with `get_state` if a widget refresh is desired.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      const r = server.next();
      return wrapResult({ moved: r.moved, state: serializeState(r.state) });
    }),
  );

  mcp.registerTool(
    "previous",
    {
      description:
        "Move to the previous item. Plain tool \u2014 does not mount a widget.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      const r = server.previous();
      return wrapResult({ moved: r.moved, state: serializeState(r.state) });
    }),
  );

  registerAppTool(
    mcp,
    "goto",
    {
      description:
        "Jump to a specific kata and section. Use the section's `id` from `list_katas` or `get_state` to identify the section. " +
        "If `sectionId` is omitted, jumps to the first section of the kata.",
      inputSchema: {
        kataId: z.string().describe("ID of the kata (e.g. 'getting_started')"),
        sectionId: z
          .string()
          .optional()
          .describe(
            "Section ID (e.g. 'getting_started__flip_qubit'). Omit to jump to the first section.",
          ),
        itemIndex: z
          .number()
          .int()
          .min(0)
          .default(0)
          .describe("0-based item index within the section"),
      },
      _meta: uiMeta,
    },
    requireInit(
      async ({
        kataId,
        sectionId,
        itemIndex,
      }: {
        kataId: string;
        sectionId?: string;
        itemIndex?: number;
      }) => {
        const state = server.goTo(kataId, sectionId, itemIndex ?? 0);
        return wrapResult({
          widgetId: mintWidgetId(),
          state: serializeState(state),
        });
      },
    ),
  );

  // ─── External navigation polling (app-only) ───

  registerAppTool(
    mcp,
    "check_navigate",
    {
      description:
        "Poll for a pending navigation request triggered by the VS Code tree view. " +
        "Returns { navigated: true, state } when a navigation occurred, or { navigated: false } otherwise. " +
        "App-only — hidden from the model.",
      inputSchema: { widgetId: z.string().optional() },
      _meta: { ui: { resourceUri: WIDGET_URI, visibility: ["app"] } },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      if (pendingNavigation) {
        const state = pendingNavigation;
        pendingNavigation = null;
        return wrapResult({ navigated: true, state });
      }
      return wrapResult({ navigated: false });
    }),
  );

  // ─── Open in full panel (app-only) ───

  registerAppTool(
    mcp,
    "open_katas_panel",
    {
      description:
        "Signal the VS Code extension host to open the full Quantum Katas panel. " +
        "Writes a signal file that the extension watches for. App-only — hidden from the model.",
      _meta: { ui: { resourceUri: WIDGET_URI, visibility: ["app"] } },
    },
    requireInit(async () => {
      if (!currentWorkspacePath) {
        return errorResponse("No workspace path configured.");
      }
      const signalPath = join(currentWorkspacePath, OPEN_PANEL_FILE);
      try {
        writeFileSync(signalPath, "", "utf-8");
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        return errorResponse(`Failed to write signal file: ${msg}`);
      }
      return wrapResult({ ok: true });
    }),
  );

  // ─── Q# execution ───

  mcp.registerTool(
    "run",
    {
      description:
        "Run the Q# code at the current position. Returns execution events and result. Plain tool \u2014 does not mount a widget.",
      inputSchema: {
        shots: z.number().int().min(1).default(1).optional(),
        widgetId: z.string().optional(),
      },
    },
    requireInit(
      async ({ shots, widgetId }: { shots?: number; widgetId?: string }) => {
        const stale = checkStale(widgetId);
        if (stale) return stale;
        const r = await server.run(shots ?? 1);
        return wrapResult({ result: r.result, state: serializeState(r.state) });
      },
    ),
  );

  mcp.registerTool(
    "run_with_noise",
    {
      description:
        "Run the Q# code with noise simulation (many shots). Plain tool \u2014 does not mount a widget.",
      inputSchema: {
        shots: z.number().int().min(1).default(100).optional(),
        widgetId: z.string().optional(),
      },
    },
    requireInit(
      async ({ shots, widgetId }: { shots?: number; widgetId?: string }) => {
        const stale = checkStale(widgetId);
        if (stale) return stale;
        const r = await server.runWithNoise(shots ?? 100);
        return wrapResult({ result: r.result, state: serializeState(r.state) });
      },
    ),
  );

  mcp.registerTool(
    "circuit",
    {
      description:
        "Generate the quantum circuit diagram for the current Q# code. Plain tool \u2014 does not mount a widget.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      const r = await server.getCircuit();
      return wrapResult({ result: r.result, state: serializeState(r.state) });
    }),
  );

  mcp.registerTool(
    "estimate",
    {
      description:
        "Estimate physical resources (qubits, runtime) for the current Q# program. Plain tool \u2014 does not mount a widget.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      const r = await server.getResourceEstimate();
      return wrapResult({ result: r.result, state: serializeState(r.state) });
    }),
  );

  mcp.registerTool(
    "check",
    {
      description:
        "Check the student's solution to the current exercise. Marks it complete on pass. Plain tool \u2014 does not mount a widget.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      const r = await server.checkSolution();
      return wrapResult({ result: r.result, state: serializeState(r.state) });
    }),
  );

  // ─── Hints / answers ───

  mcp.registerTool(
    "hint",
    {
      description:
        "Reveal the next built-in hint for the current exercise. Plain tool \u2014 does not mount a widget.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      const r = server.getNextHint();
      return wrapResult({ result: r.result, state: serializeState(r.state) });
    }),
  );

  mcp.registerTool(
    "reveal_answer",
    {
      description:
        "Reveal the answer to the current lesson question. Plain tool \u2014 does not mount a widget.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      const r = server.revealAnswer();
      return wrapResult({ result: r.result, state: serializeState(r.state) });
    }),
  );

  mcp.registerTool(
    "solution",
    {
      description:
        "Show the full reference solution code for the current exercise. Plain tool \u2014 does not mount a widget.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      return wrapResult({
        result: server.getFullSolution(),
        state: serializeState(server.getState()),
      });
    }),
  );

  // ─── AI tools (use MCP sampling to call back into the host's model) ───

  mcp.registerTool(
    "ai_hint",
    {
      description:
        "Get an AI-generated hint tailored to the student's current code. Uses MCP sampling to ask the host's model. Plain tool \u2014 does not mount a widget.",
      inputSchema: { widgetId: z.string().optional() },
    },
    requireInit(async ({ widgetId }: { widgetId?: string }) => {
      const stale = checkStale(widgetId);
      if (stale) return stale;
      const r = await server.getAIHint();
      return wrapResult({ result: r.result, state: serializeState(r.state) });
    }),
  );

  mcp.registerTool(
    "ask_ai",
    {
      description:
        "Ask a free-form quantum computing question about the current lesson. Uses MCP sampling. Plain tool \u2014 does not mount a widget.",
      inputSchema: {
        question: z.string().min(1).describe("The student's question"),
      },
    },
    requireInit(async ({ question }: { question: string }) => {
      process.stderr.write(
        `[katas-mcp] ask_ai invoked: question=${JSON.stringify(question).slice(0, 200)}\n`,
      );
      const r = await server.askConceptQuestion(question);
      process.stderr.write(
        `[katas-mcp] ask_ai result: ${r.result === null ? "null" : `${r.result.length} chars`}\n`,
      );
      return wrapResult({ result: r.result, state: serializeState(r.state) });
    }),
  );
}

/**
 * Register katas handlers and connect the MCP server over stdio.
 * Stdout is the wire — absolutely no console.log is permitted once we return.
 */
export async function runMCPServerStdio(
  server: KatasServer,
  mcp: McpServer,
  initOptions: {
    aiProvider: IAIProvider;
    contentFormat: "html" | "markdown";
    /** See {@link registerMCPHandlers}. */
    initialWorkspace?: string;
    /** See {@link registerMCPHandlers}. */
    initialKatasRoot?: string;
  },
): Promise<void> {
  await registerMCPHandlers(server, mcp, initOptions);
  const transport = new StdioServerTransport();
  await mcp.connect(transport);
}

/**
 * Register katas handlers and serve the MCP server over Streamable HTTP.
 *
 * The HTTP deployment uses the same `registerMCPHandlers` implementation as
 * stdio — only the transport and session management differ, so the two
 * cannot drift in behavior.
 *
 * Multiple concurrent sessions are supported. For each new `initialize`
 * request (POST without an `mcp-session-id` header) a fresh `McpServer` and
 * transport are created and `registerMCPHandlers` is called, giving each
 * client its own closure state (live widget id, initialization gate,
 * current workspace). All sessions share the same underlying `KatasServer`
 * (the kata engine / workspace / progress).
 *
 * The caller supplies factories so HTTP-specific fresh-per-session MCP
 * servers and sampling AI providers can be constructed lazily.
 */
export async function runMCPServerHttp(
  server: KatasServer,
  factories: {
    createMcpServer: () => McpServer;
    /** Construct an AI provider bound to the given McpServer instance. */
    createAIProvider: (mcp: McpServer) => IAIProvider;
    contentFormat: "html" | "markdown";
  },
  httpOptions: {
    port: number;
    host?: string;
    path?: string;
    /**
     * CORS allow-list for the `Access-Control-Allow-Origin` header.
     * - `undefined` (default): no CORS headers (same-origin / MCP hosts that
     *   don't use a browser fetch).
     * - `"*"`: allow any origin (cannot be combined with credentialed requests).
     * - `string[]`: allow only the listed origins, echoed back on match.
     */
    allowedOrigins?: "*" | string[];
  },
): Promise<{ close: () => Promise<void> }> {
  const path = httpOptions.path ?? "/mcp";
  const host = httpOptions.host ?? "127.0.0.1";
  const allowedOrigins = httpOptions.allowedOrigins;

  // sessionId -> { transport, mcp }
  const sessions = new Map<
    string,
    { transport: StreamableHTTPServerTransport; mcp: McpServer }
  >();

  async function createSession(): Promise<StreamableHTTPServerTransport> {
    const mcp = factories.createMcpServer();
    const aiProvider = factories.createAIProvider(mcp);
    await registerMCPHandlers(server, mcp, {
      aiProvider,
      contentFormat: factories.contentFormat,
    });

    const transport = new StreamableHTTPServerTransport({
      sessionIdGenerator: () => randomUUID(),
      onsessioninitialized: (sessionId: string) => {
        sessions.set(sessionId, { transport, mcp });
        process.stderr.write(
          `[katas-mcp] HTTP session opened: ${sessionId} (active=${sessions.size})\n`,
        );
      },
      onsessionclosed: (sessionId: string) => {
        sessions.delete(sessionId);
        process.stderr.write(
          `[katas-mcp] HTTP session closed: ${sessionId} (active=${sessions.size})\n`,
        );
        // Best-effort cleanup of the McpServer for this session.
        void mcp.close?.();
      },
    } as ConstructorParameters<typeof StreamableHTTPServerTransport>[0]);

    transport.onclose = () => {
      if (transport.sessionId) {
        sessions.delete(transport.sessionId);
      }
      void mcp.close?.();
    };

    await mcp.connect(transport);
    return transport;
  }

  function applyCors(req: IncomingMessage, res: ServerResponse): void {
    if (!allowedOrigins) return;
    const origin = req.headers.origin;
    let allow: string | null = null;
    if (allowedOrigins === "*") {
      allow = "*";
    } else if (typeof origin === "string" && allowedOrigins.includes(origin)) {
      allow = origin;
    }
    if (allow) {
      res.setHeader("access-control-allow-origin", allow);
      if (allow !== "*") res.setHeader("vary", "origin");
      res.setHeader(
        "access-control-allow-headers",
        "content-type, mcp-session-id, mcp-protocol-version, authorization, last-event-id",
      );
      res.setHeader(
        "access-control-expose-headers",
        "mcp-session-id, mcp-protocol-version",
      );
      res.setHeader(
        "access-control-allow-methods",
        "GET, POST, DELETE, OPTIONS",
      );
      res.setHeader("access-control-max-age", "86400");
    }
  }

  const httpServer = createHttpServerNode(
    async (req: IncomingMessage, res: ServerResponse) => {
      applyCors(req, res);

      // Only the configured path is served; anything else gets 404.
      const url = req.url ?? "";
      const qIdx = url.indexOf("?");
      const reqPath = qIdx >= 0 ? url.slice(0, qIdx) : url;
      if (reqPath !== path) {
        res.statusCode = 404;
        res.setHeader("content-type", "text/plain");
        res.end("Not Found");
        return;
      }

      // Answer CORS preflights directly; don't hand OPTIONS to the transport.
      if (req.method === "OPTIONS") {
        res.statusCode = 204;
        res.end();
        return;
      }

      try {
        const sidHeader = req.headers["mcp-session-id"];
        const sid = Array.isArray(sidHeader) ? sidHeader[0] : sidHeader;

        let transport: StreamableHTTPServerTransport;
        if (sid && sessions.has(sid)) {
          transport = sessions.get(sid)!.transport;
        } else if (!sid && req.method === "POST") {
          // New session — the transport will accept `initialize` and mint a
          // session id via `onsessioninitialized`. Non-initialize POSTs
          // without a session id will be rejected by the transport itself.
          transport = await createSession();
        } else {
          res.statusCode = 404;
          res.setHeader("content-type", "application/json");
          res.end(
            JSON.stringify({
              jsonrpc: "2.0",
              error: {
                code: -32001,
                message: sid
                  ? `Unknown session id: ${sid}`
                  : "Missing mcp-session-id header",
              },
              id: null,
            }),
          );
          return;
        }

        await transport.handleRequest(req, res);
      } catch (err) {
        process.stderr.write(
          `[katas-mcp] HTTP handler error: ${err instanceof Error ? (err.stack ?? err.message) : String(err)}\n`,
        );
        if (!res.headersSent) {
          res.statusCode = 500;
          res.setHeader("content-type", "text/plain");
          res.end("Internal Server Error");
        } else {
          try {
            res.end();
          } catch {
            /* already closed */
          }
        }
      }
    },
  );

  await new Promise<void>((resolvePromise, rejectPromise) => {
    httpServer.once("error", rejectPromise);
    httpServer.listen(httpOptions.port, host, () => {
      httpServer.off("error", rejectPromise);
      resolvePromise();
    });
  });

  return {
    close: async () => {
      for (const { transport, mcp } of sessions.values()) {
        try {
          await transport.close();
        } catch {
          /* ignore */
        }
        try {
          await mcp.close?.();
        } catch {
          /* ignore */
        }
      }
      sessions.clear();
      await new Promise<void>((res) => httpServer.close(() => res()));
    },
  };
}

export { McpServer };
