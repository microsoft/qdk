// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

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
import { z } from "zod";

export function createServer(): McpServer {
  const server = new McpServer({
    name: "QDK MCP Server",
    version: "0.0.1",
  });

  const circuitUri = "ui://qdk/circuit-app.html";

  // --- renderCircuit tool ---

  registerAppTool(
    server,
    "renderCircuit",
    {
      title: "Render Circuit",
      description:
        "Render a quantum circuit diagram from JSON circuit data. " +
        "Accepts either a CircuitGroup object ({ circuits: [...] }) or " +
        "a bare Circuit object ({ qubits: [...], componentGrid: [...] }). " +
        "Use this to visualize circuit data obtained from the QDK Python library " +
        "(e.g. qsharp.circuit(...).json()).",
      inputSchema: z.object({
        circuitJson: z
          .string()
          .describe(
            "JSON string representing a Circuit or CircuitGroup object, " +
            "as returned by the Python qsharp.circuit().json() method.",
          ),
      }),
      _meta: { ui: { resourceUri: circuitUri } },
    },
    async (args: { circuitJson: string }): Promise<CallToolResult> => {
      let parsed: Record<string, unknown>;
      try {
        parsed = JSON.parse(args.circuitJson) as Record<string, unknown>;
      } catch {
        return {
          isError: true,
          content: [
            {
              type: "text",
              text: "Invalid JSON: the circuitJson input could not be parsed.",
            },
          ],
        };
      }

      // Normalize: accept both a bare Circuit ({ qubits, componentGrid })
      // and a full CircuitGroup ({ version, circuits: [...] }).
      // The circuit-app's toCircuitGroup() handles both, but we normalize
      // here so the summary stats are correct.
      let circuitData: Record<string, unknown>;
      if (
        typeof parsed.version === "number" &&
        Array.isArray(parsed.circuits)
      ) {
        // Already a CircuitGroup
        circuitData = parsed;
      } else if (
        Array.isArray(parsed.qubits) &&
        Array.isArray(parsed.componentGrid)
      ) {
        // Bare Circuit — wrap into a CircuitGroup with version
        circuitData = { version: 1, circuits: [parsed] };
      } else {
        circuitData = parsed;
      }

      // Extract stats from the first circuit for the summary text
      const circuits = circuitData.circuits;
      let numQubits = 0;
      let numOps = 0;
      if (Array.isArray(circuits) && circuits.length > 0) {
        const first = circuits[0] as Record<string, unknown>;
        numQubits = Array.isArray(first.qubits) ? first.qubits.length : 0;
        numOps = Array.isArray(first.componentGrid)
          ? first.componentGrid.length
          : 0;
      }

      return {
        structuredContent: circuitData,
        content: [
          {
            type: "text",
            text: `Circuit rendered: ${numQubits} qubit(s), ${numOps} gate column(s).`,
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
        path.join(__dirname, "circuit-app.html"),
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
