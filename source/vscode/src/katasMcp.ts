// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { log } from "qsharp-lang";
import { EventType, sendTelemetryEvent } from "./telemetry";

/**
 * Registers a static MCP server definition provider for the bundled
 * Quantum Katas MCP server. The server is a Node CLI bundled at
 * `out/learning/index.js`; we spawn it in `--mcp` (stdio) mode and
 * pass the configured workspace root (if any) via `--workspace`.
 *
 * Workspace-only / desktop-only: VS Code treats a static MCP server
 * provider contributed from a Node-platform extension entry point
 * as desktop-only by default, so no extra gating is needed here.
 */
export function registerKatasMcpServer(
  context: vscode.ExtensionContext,
): vscode.Disposable {
  const lm = vscode.lm as unknown as {
    registerMcpServerDefinitionProvider?: (
      id: string,
      provider: McpServerDefinitionProvider,
    ) => vscode.Disposable;
  };

  if (typeof lm.registerMcpServerDefinitionProvider !== "function") {
    log.warn(
      "vscode.lm.registerMcpServerDefinitionProvider not available; " +
        "Quantum Katas MCP server will not be registered.",
    );
    return new vscode.Disposable(() => {});
  }

  const onDidChangeEmitter = new vscode.EventEmitter<void>();

  const provider: McpServerDefinitionProvider = {
    onDidChangeMcpServerDefinitions: onDidChangeEmitter.event,
    provideMcpServerDefinitions: () => {
      const cfg = vscode.workspace.getConfiguration("Q#");
      const workspaceRoot = (
        cfg.get<string>("learning.workspaceRoot") ?? ""
      ).trim();

      const entry = vscode.Uri.joinPath(
        context.extensionUri,
        "out",
        "learning",
        "index.js",
      ).fsPath;

      const args = [entry, "--mcp"];
      if (workspaceRoot.length > 0) {
        args.push("--workspace", workspaceRoot);
      }

      sendTelemetryEvent(
        EventType.QuantumKatasMcpStart,
        {
          workspaceRootConfigured: workspaceRoot.length > 0 ? "true" : "false",
        },
        {},
      );

      return [
        new (
          vscode as unknown as { McpStdioServerDefinition: any }
        ).McpStdioServerDefinition("Quantum Katas", process.execPath, args, {}),
      ];
    },
  };

  const disposable = lm.registerMcpServerDefinitionProvider(
    "qdk.quantum-katas",
    provider,
  );

  // Refresh the definition when the configured workspace root changes,
  // so the server is restarted with the new --workspace arg.
  const cfgListener = vscode.workspace.onDidChangeConfiguration((e) => {
    if (e.affectsConfiguration("Q#.learning.workspaceRoot")) {
      onDidChangeEmitter.fire();
    }
  });

  return vscode.Disposable.from(disposable, cfgListener, onDidChangeEmitter);
}

interface McpServerDefinitionProvider {
  readonly onDidChangeMcpServerDefinitions?: vscode.Event<void>;
  provideMcpServerDefinitions: () =>
    | vscode.ProviderResult<unknown[]>
    | Thenable<unknown[]>;
}
