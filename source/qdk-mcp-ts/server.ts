import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type {
  CallToolResult,
  ReadResourceResult,
} from "@modelcontextprotocol/sdk/types.js";
import fs from "node:fs/promises";
import path from "node:path";
import {
  registerAppTool,
  registerAppResource,
  RESOURCE_MIME_TYPE,
} from "@modelcontextprotocol/ext-apps/server";
import { getCompiler } from "qsharp-lang";
import { z } from "zod";

type Compiler = ReturnType<typeof getCompiler>;

// Works both from source (server.ts) and compiled (dist/server.js)
const DIST_DIR = import.meta.filename.endsWith(".ts")
  ? path.join(import.meta.dirname, "dist")
  : import.meta.dirname;

let compiler: Compiler | undefined;

function ensureCompiler(): Compiler {
  if (!compiler) {
    compiler = getCompiler();
  }
  return compiler;
}

export function createServer(): McpServer {
  const server = new McpServer({
    name: "QDK MCP Server",
    version: "0.0.1",
  });

  // --- get-time tool (baseline) ---

  const getTimeUri = "ui://qdk/mcp-app.html";

  registerAppTool(
    server,
    "get-time",
    {
      title: "Get Time",
      description: "Returns the current server time as an ISO 8601 string.",
      inputSchema: {},
      _meta: { ui: { resourceUri: getTimeUri } },
    },
    async (): Promise<CallToolResult> => {
      const time = new Date().toISOString();
      return { content: [{ type: "text", text: time }] };
    },
  );

  registerAppResource(
    server,
    getTimeUri,
    getTimeUri,
    { mimeType: RESOURCE_MIME_TYPE },
    async (): Promise<ReadResourceResult> => {
      const html = await fs.readFile(
        path.join(DIST_DIR, "mcp-app.html"),
        "utf-8",
      );
      return {
        contents: [
          { uri: getTimeUri, mimeType: RESOURCE_MIME_TYPE, text: html },
        ],
      };
    },
  );

  // --- circuit tool ---

  const circuitUri = "ui://qdk/circuit-app.html";

  registerAppTool(
    server,
    "circuit",
    {
      title: "Q# Circuit",
      description:
        "Generate a quantum circuit diagram from Q# source code. Returns the circuit as structured JSON data and renders it visually.",
      inputSchema: z.object({
        source: z
          .string()
          .describe("Q# source code to generate a circuit from."),
        generationMethod: z
          .enum(["simulate", "classicalEval", "static"])
          .optional()
          .describe(
            'Circuit generation method. "simulate" traces execution, "classicalEval" evaluates classical logic, "static" performs static analysis. Defaults to "simulate".',
          ),
      }),
      _meta: { ui: { resourceUri: circuitUri } },
    },
    async (args: {
      source: string;
      generationMethod?: "simulate" | "classicalEval" | "static";
    }): Promise<CallToolResult> => {
      const comp = ensureCompiler();

      const program = {
        sources: [["main.qs", args.source]] as [string, string][],
        languageFeatures: [] as string[],
      };

      const config = {
        generationMethod: args.generationMethod ?? "simulate",
        maxOperations: 10000,
        sourceLocations: false,
        groupByScope: false,
      };

      const circuitData = await comp.getCircuit(program, config);

      const numQubits = circuitData.circuits[0]?.qubits?.length ?? 0;
      const numOps = circuitData.circuits[0]?.componentGrid?.length ?? 0;

      return {
        structuredContent: circuitData as unknown as Record<string, unknown>,
        content: [
          {
            type: "text",
            text: `Circuit generated: ${numQubits} qubit(s), ${numOps} gate column(s).`,
          },
        ],
      };
    },
  );

  registerAppResource(
    server,
    circuitUri,
    circuitUri,
    { mimeType: RESOURCE_MIME_TYPE },
    async (): Promise<ReadResourceResult> => {
      const html = await fs.readFile(
        path.join(DIST_DIR, "circuit-app.html"),
        "utf-8",
      );
      return {
        contents: [
          { uri: circuitUri, mimeType: RESOURCE_MIME_TYPE, text: html },
        ],
      };
    },
  );

  return server;
}
