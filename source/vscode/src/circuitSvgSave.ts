// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import {
  getCircuitSvgSaveMessageError,
  isCircuitSvgSaveMessage,
  isCircuitSvgSaveRequest,
  type CircuitSvgSaveMessage,
} from "./circuitSvgProtocol.js";

export async function handleCircuitSvgSaveMessage(
  message: unknown,
): Promise<boolean> {
  if (!isCircuitSvgSaveRequest(message)) {
    return false;
  }

  const validationError = getCircuitSvgSaveMessageError(message);
  if (validationError !== undefined || !isCircuitSvgSaveMessage(message)) {
    await reportCircuitSvgSaveError(
      new Error(validationError ?? "The SVG export request is invalid."),
    );
    return true;
  }

  try {
    await saveCircuitSvg(message);
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

async function saveCircuitSvg(message: CircuitSvgSaveMessage): Promise<void> {
  const folder = vscode.workspace.workspaceFolders?.[0]?.uri;
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
