// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Desktop entrypoint: activates all Q# features (language service, debugger,
// notebooks, circuit editor, Azure, etc.) then registers the MCP server.

import * as vscode from "vscode";
import { activate as activateShared, ExtensionApi } from "../extension.js";

export type { ExtensionApi };

export async function activate(
  context: vscode.ExtensionContext,
): Promise<ExtensionApi> {
  // Activate all shared Q# features
  const api = await activateShared(context);

  return api;
}

export function deactivate() {
  // nothing to do
}
