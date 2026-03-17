// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Desktop entrypoint: activates all Q# features (language service, debugger,
// notebooks, circuit editor, Azure, etc.) then registers the MCP server.

import * as vscode from "vscode";
import { activate as activateShared, ExtensionApi } from "../extension.js";
import { registerLanguageModelTools } from "./copilot-tools/tools.js";

export type { ExtensionApi };

export async function activate(
  context: vscode.ExtensionContext,
): Promise<ExtensionApi> {
  // Activate all shared Q# features
  const api = await activateShared(context);

  // Register language model tools (desktop-only)
  registerLanguageModelTools(context);

  // Register the MCP server (desktop-only)
  const serverPath = context.asAbsolutePath("out/desktop/mcp/server.js");
  const disposable = vscode.lm.registerMcpServerDefinitionProvider("qdk", {
    provideMcpServerDefinitions: () => [
      new vscode.McpStdioServerDefinition("QDK", process.execPath, [
        serverPath,
      ]),
    ],
    onDidChangeMcpServerDefinitions: new vscode.EventEmitter<void>().event,
  });
  context.subscriptions.push(disposable);

  return api;
}

export function deactivate() {
  // nothing to do
}
