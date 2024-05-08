// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { TextDocument, Uri, Range, Location } from "vscode";
import { ILocation, IRange, IWorkspaceEdit } from "qsharp-lang";
import * as vscode from "vscode";

export const qsharpLanguageId = "qsharp";

// Returns true for all Q# documents, including unsaved files, notebook cells, etc.
export function isQsharpDocument(document: TextDocument): boolean {
  return document.languageId === qsharpLanguageId;
}

// Returns true for only Q# notebook cell documents.
export function isQsharpNotebookCell(document: TextDocument): boolean {
  return (
    document.languageId === qsharpLanguageId &&
    document.uri.scheme === "vscode-notebook-cell"
  );
}

export const qsharpExtensionId = "qsharp-vscode";

export function basename(path: string): string | undefined {
  return path.replace(/\/+$/, "").split("/").pop();
}

export function toVscodeRange(range: IRange): Range {
  return new Range(
    range.start.line,
    range.start.character,
    range.end.line,
    range.end.character,
  );
}

export function toVscodeLocation(location: ILocation): any {
  return new Location(Uri.parse(location.source), toVscodeRange(location.span));
}

export function toVscodeWorkspaceEdit(
  iWorkspaceEdit: IWorkspaceEdit,
): vscode.WorkspaceEdit {
  const workspaceEdit = new vscode.WorkspaceEdit();
  for (const [source, edits] of iWorkspaceEdit.changes) {
    const uri = vscode.Uri.parse(source, true);
    const vsEdits = edits.map((edit) => {
      return new vscode.TextEdit(toVscodeRange(edit.range), edit.newText);
    });
    workspaceEdit.set(uri, vsEdits);
  }
  return workspaceEdit;
}
