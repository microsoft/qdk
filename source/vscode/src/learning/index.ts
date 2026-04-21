// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { resolve } from "node:path";
import { parseArgs } from "node:util";
import { KatasServer, LLMAIProvider, NoOpAIProvider } from "./server/index.js";
import { runApp } from "./tui/index.js";
import { createHttpServer } from "./web/index.js";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { runMCPServerStdio, runMCPServerHttp } from "./mcp/server.js";
import { MCPSamplingAIProvider } from "./mcp/ai-provider.js";

const { values } = parseArgs({
  options: {
    katas: { type: "string", short: "k", multiple: true, default: [] },
    workspace: { type: "string", short: "w", default: "." },
    web: { type: "boolean", default: false },
    mcp: { type: "boolean", default: false },
    "mcp-http": { type: "boolean", default: false },
    "mcp-host": { type: "string", default: "127.0.0.1" },
    "mcp-path": { type: "string", default: "/mcp" },
    "mcp-allow-origin": { type: "string", multiple: true, default: [] },
    port: { type: "string", default: "3000" },
    "ai-endpoint": { type: "string" },
    "ai-key": { type: "string" },
    "ai-model": { type: "string", default: "gpt-4o" },
    help: { type: "boolean", short: "h", default: false },
  },
  strict: true,
});

if (values.help) {
  console.log(`
Usage: katas-tui [options]

Options:
  -k, --katas <id>       Kata IDs to load (can repeat; loads all if omitted)
  -w, --workspace <path> Workspace directory (default: current directory)
  --web                  Start web UI instead of terminal UI
  --mcp                  Run as an MCP server over stdio (mutually exclusive with --web/--mcp-http)
  --mcp-http             Run as an MCP server over Streamable HTTP (mutually exclusive with --web/--mcp)
  --mcp-host <host>      Bind host for --mcp-http (default: 127.0.0.1)
  --mcp-path <path>      URL path for --mcp-http (default: /mcp)
  --mcp-allow-origin <o> CORS allow-list for --mcp-http (repeatable; use "*" for any)
  --port <port>          Port for web or MCP HTTP server (default: 3000)
  --ai-endpoint <url>    OpenAI-compatible API endpoint for AI features
  --ai-key <key>         API key for the AI endpoint
  --ai-model <model>     Model name (default: gpt-4o)
  -h, --help             Show this help message

Examples:
  katas-tui
  katas-tui --web
  katas-tui --web --port 8080
  katas-tui --mcp
  katas-tui --mcp-http --port 3457
  katas-tui -k getting_started -k complex_arithmetic
  katas-tui --ai-endpoint https://api.openai.com/v1 --ai-key sk-...
`);
  process.exit(0);
}

const useWeb = values.web ?? false;
const useMcpStdio = values.mcp ?? false;
const useMcpHttp = values["mcp-http"] ?? false;
const useMcp = useMcpStdio || useMcpHttp;
if ([useWeb, useMcpStdio, useMcpHttp].filter(Boolean).length > 1) {
  console.error("Error: --web, --mcp, and --mcp-http are mutually exclusive");
  process.exit(1);
}

const port = parseInt(values.port ?? "3000", 10);
// In MCP mode we defer initialization until the agent calls `set_workspace` —
// so the raw CLI values are handed through without resolving a default.
const workspacePath = useMcp ? undefined : resolve(values.workspace ?? ".");
const kataIds = (values.katas ?? []) as string[];

// In MCP mode, prefer sampling-based AI (no API key needed); fall back to
// LLM provider if the user explicitly configured one; else NoOp.
// For --mcp (stdio) there is exactly one McpServer; for --mcp-http a fresh
// McpServer + sampling AI provider is created per session, so we build
// factories below rather than a single instance.
let aiProvider;
let mcpServer: McpServer | undefined;
if (useMcpStdio) {
  mcpServer = new McpServer({ name: "qsharp-katas", version: "0.1.0" });
  aiProvider =
    values["ai-endpoint"] && values["ai-key"]
      ? new LLMAIProvider({
          endpoint: values["ai-endpoint"],
          apiKey: values["ai-key"],
          model: values["ai-model"] ?? "gpt-4o",
        })
      : new MCPSamplingAIProvider(mcpServer);
} else if (useMcpHttp) {
  // Per-session factories are built inline where runMCPServerHttp is called.
  // aiProvider stays undefined here; it's only used by stdio/web/TUI.
} else {
  aiProvider =
    values["ai-endpoint"] && values["ai-key"]
      ? new LLMAIProvider({
          endpoint: values["ai-endpoint"],
          apiKey: values["ai-key"],
          model: values["ai-model"] ?? "gpt-4o",
        })
      : new NoOpAIProvider();
}

const hasAI = !(aiProvider instanceof NoOpAIProvider);

const server = new KatasServer();

try {
  if (useMcpStdio) {
    // Stdout is the MCP wire — absolutely no console.log here.
    // KatasServer is NOT initialized yet; the agent must call `set_workspace`
    // first, which will elicit user confirmation before initializing.
    await runMCPServerStdio(server, mcpServer!, {
      aiProvider: aiProvider!,
      contentFormat: "html",
    });
    process.on("SIGINT", () => {
      server.dispose();
      process.exit(0);
    });
  } else if (useMcpHttp) {
    // HTTP MCP — logging to stdout is fine here (stdout is not the wire).
    // Same handler registration as stdio, but each session gets its own
    // McpServer + sampling AI provider so multiple/re-connecting clients
    // don't trip the "server already initialized" guard.
    const rawOrigins = (values["mcp-allow-origin"] ?? []) as string[];
    const allowedOrigins: "*" | string[] | undefined =
      rawOrigins.length === 0
        ? undefined
        : rawOrigins.includes("*")
          ? "*"
          : rawOrigins;
    const llmEndpoint = values["ai-endpoint"];
    const llmKey = values["ai-key"];
    const llmModel = values["ai-model"] ?? "gpt-4o";
    const httpMcp = await runMCPServerHttp(
      server,
      {
        createMcpServer: () =>
          new McpServer({ name: "qsharp-katas", version: "0.1.0" }),
        createAIProvider: (mcp) =>
          llmEndpoint && llmKey
            ? new LLMAIProvider({
                endpoint: llmEndpoint,
                apiKey: llmKey,
                model: llmModel,
              })
            : new MCPSamplingAIProvider(mcp),
        contentFormat: "html",
      },
      {
        port,
        host: values["mcp-host"] ?? "127.0.0.1",
        path: values["mcp-path"] ?? "/mcp",
        allowedOrigins,
      },
    );
    const host = values["mcp-host"] ?? "127.0.0.1";
    const mcpPath = values["mcp-path"] ?? "/mcp";
    console.log(`Katas MCP server running at http://${host}:${port}${mcpPath}`);
    process.on("SIGINT", async () => {
      await httpMcp.close();
      server.dispose();
      process.exit(0);
    });
  } else if (useWeb) {
    await server.initialize({
      kataIds,
      workspacePath: workspacePath!,
      aiProvider,
      contentFormat: "html",
    });
    const httpServer = createHttpServer(server);
    httpServer.listen(port, () => {
      console.log(`Katas web UI running at http://localhost:${port}`);
    });
    // Keep alive until process is killed
    process.on("SIGINT", () => {
      server.dispose();
      process.exit(0);
    });
  } else {
    await server.initialize({
      kataIds,
      workspacePath: workspacePath!,
      aiProvider,
      contentFormat: "markdown",
    });
    await runApp(server, hasAI);
    server.dispose();
  }
} catch (err) {
  if (useMcp) {
    // Can't write to stdout in MCP mode — use stderr.
    console.error("Fatal error:", err);
  } else {
    console.error("Fatal error:", err);
  }
  process.exit(1);
}
