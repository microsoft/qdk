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
}

export function deactivate() {
  // nothing to do
}
