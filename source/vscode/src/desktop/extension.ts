// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";

export function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand("qsharp-vscode.helloWorld", () => {
      vscode.window.showInformationMessage(
        "Hello World from the Q# desktop extension host!",
      );
    }),
  );

  const serverPath = context.asAbsolutePath("out/desktop/mcp/server.js");
  const disposable = vscode.lm.registerMcpServerDefinitionProvider("qdk", {
    provideMcpServerDefinitions: () => [
      new vscode.McpStdioServerDefinition("QDK", "node", [serverPath]),
    ],
    onDidChangeMcpServerDefinitions: new vscode.EventEmitter<void>().event,
  });
  context.subscriptions.push(disposable);
}

export function deactivate() {
  // nothing to do
}
