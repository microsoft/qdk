// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// DEAD CODE: This MCP server registration has been replaced by in-proc
// qdk-learning-* LM tools (see gh-copilot/learningTools.ts). This file and
// the learning/ CLI bundle will be deleted in a follow-up change.

import * as vscode from "vscode";
import { log } from "qsharp-lang";
import { detectKatasWorkspace, LEARNING_FILE } from "./katasProgress/detector";
import { EventType, sendTelemetryEvent } from "./telemetry";

/**
 * Registers a static MCP server definition provider for the bundled
 * Quantum Katas MCP server. The server is a Node CLI bundled at
 * `out/learning/index.js`; we spawn it in `--mcp` (stdio) mode and
 * pass the discovered workspace root via `--workspace`.
 *
 * The workspace root is whatever {@link detectKatasWorkspace} returns —
 * a workspace folder containing a `qdk-learning.json` file.
 * When the discovered path already has a katas root folder,
 * the CLI eagerly initializes the server, so the chat agent does not
 * have to call `init`.
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

  // Refresh the definition when the set of open workspace folders changes or
  // when a `qdk-learning.json` file is created/deleted, so the server restarts
  // pointing at the right path.
  const foldersListener = vscode.workspace.onDidChangeWorkspaceFolders(() => {
    onDidChangeEmitter.fire();
  });

  const learningFileWatcher = vscode.workspace.createFileSystemWatcher(
    `**/${LEARNING_FILE}`,
  );
  const onLearningFileEvent = () => onDidChangeEmitter.fire();
  learningFileWatcher.onDidCreate(onLearningFileEvent);
  learningFileWatcher.onDidDelete(onLearningFileEvent);
  learningFileWatcher.onDidChange(onLearningFileEvent);

  // Watch for the `.open-panel` signal file written by the MCP server's
  // `open_katas_panel` tool. When it appears, open the full katas panel
  // and delete the signal file.
  const OPEN_PANEL_FILE = ".open-panel";
  const openPanelWatcher = vscode.workspace.createFileSystemWatcher(
    `**/${OPEN_PANEL_FILE}`,
  );
  const onOpenPanelSignal = async (uri: vscode.Uri) => {
    try {
      await vscode.workspace.fs.delete(uri);
    } catch {
      // File may already be gone.
    }
    await vscode.commands.executeCommand("qsharp-vscode.showKatas");
  };
  openPanelWatcher.onDidCreate(onOpenPanelSignal);
  openPanelWatcher.onDidChange(onOpenPanelSignal);

  return vscode.Disposable.from(
    disposable,
    foldersListener,
    learningFileWatcher,
    openPanelWatcher,
    onDidChangeEmitter,
  );
}

interface McpServerDefinitionProvider {
  readonly onDidChangeMcpServerDefinitions?: vscode.Event<void>;
  provideMcpServerDefinitions: () =>
    | vscode.ProviderResult<unknown[]>
    | Thenable<unknown[]>;
}
