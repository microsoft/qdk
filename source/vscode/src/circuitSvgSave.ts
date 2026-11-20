// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import {
  getCircuitSvgSaveMessageError,
  isCircuitSvgSaveRequest,
  type CircuitSvgSaveMessage,
} from "./circuitSvgProtocol.js";

export async function handleCircuitSvgSaveMessage(
  message: unknown,
  sourceUri?: vscode.Uri,
): Promise<boolean> {
  if (!isCircuitSvgSaveRequest(message)) {
    return false;
  }

  const validationError = getCircuitSvgSaveMessageError(message);
  if (validationError !== undefined) {
    await reportCircuitSvgSaveError(new Error(validationError));
    return true;
  }

  try {
    await saveCircuitSvg(message as CircuitSvgSaveMessage, sourceUri);
  } catch (error) {
    await reportCircuitSvgSaveError(error);
  }
  return true;
}

async function reportCircuitSvgSaveError(error: unknown): Promise<void> {
  log.error("Circuit SVG export failed.", error);
  await vscode.window.showErrorMessage(
    error instanceof Error
      ? `Circuit SVG export failed: ${error.message}`
      : "Circuit SVG export failed.",
  );
}

async function saveCircuitSvg(
  message: CircuitSvgSaveMessage,
  sourceUri: vscode.Uri | undefined,
): Promise<void> {
  // Without a workspace folder, start in the circuit's source folder rather than the file system
  // root.
  const source = sourceUri ?? vscode.window.activeTextEditor?.document.uri;
  const folder =
    vscode.workspace.workspaceFolders?.[0]?.uri ??
    (source?.scheme === "file" ? vscode.Uri.joinPath(source, "..") : undefined);
  const target = await vscode.window.showSaveDialog({
    defaultUri: folder
      ? vscode.Uri.joinPath(folder, message.suggestedName)
      : vscode.Uri.file(message.suggestedName),
    filters: { "SVG image": ["svg"] },
    title: "Export circuit as SVG",
  });
  if (target === undefined) {
    return;
  }

  await vscode.workspace.fs.writeFile(
    target,
    new TextEncoder().encode(message.contents),
  );
  vscode.window.setStatusBarMessage(
    `Exported ${vscode.workspace.asRelativePath(target)}`,
    4000,
  );
}
