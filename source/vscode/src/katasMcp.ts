// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { log } from "qsharp-lang";
import { detectKatasWorkspace } from "./katasProgress/detector";
import { EventType, sendTelemetryEvent } from "./telemetry";

/**
 * Registers a static MCP server definition provider for the bundled
 * Quantum Katas MCP server. The server is a Node CLI bundled at
 * `out/learning/index.js`; we spawn it in `--mcp` (stdio) mode and
 * pass the discovered workspace root via `--workspace`.
 *
 * The workspace root is whatever {@link detectKatasWorkspace} returns:
 * either the explicit `Q#.learning.workspaceRoot` setting, or a
 * workspace folder containing an existing `quantum-katas/` directory.
 * When the discovered path already has a `quantum-katas/` subfolder,
 * the CLI eagerly initializes the server, so the chat agent does not
 * have to call `set_workspace`.
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
    provideMcpServerDefinitions: async () => {
      const info = await detectKatasWorkspace();
      const workspaceRoot = info?.workspaceRoot.fsPath ?? "";

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

  // Refresh the definition when either the configured workspace root or the
  // set of open workspace folders changes, so the server restarts pointing
  // at the right path.
  const cfgListener = vscode.workspace.onDidChangeConfiguration((e) => {
    if (e.affectsConfiguration("Q#.learning.workspaceRoot")) {
      onDidChangeEmitter.fire();
    }
  });
  const foldersListener = vscode.workspace.onDidChangeWorkspaceFolders(() => {
    onDidChangeEmitter.fire();
  });

  return vscode.Disposable.from(
    disposable,
    cfgListener,
    foldersListener,
    onDidChangeEmitter,
  );
}

interface McpServerDefinitionProvider {
  readonly onDidChangeMcpServerDefinitions?: vscode.Event<void>;
  provideMcpServerDefinitions: () =>
    | vscode.ProviderResult<unknown[]>
    | Thenable<unknown[]>;
}
