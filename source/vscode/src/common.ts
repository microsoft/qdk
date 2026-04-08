// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
declare const __PLATFORM__: string;

import { TextDocument, Uri, Range, Location } from "vscode";
import {
  getCompilerWorker,
  ICompilerWorker,
  ILocation,
  IRange,
  IWorkspaceEdit,
  VSDiagnostic,
} from "qsharp-lang";
import * as vscode from "vscode";

export const qsharpLanguageId = "qsharp";
export const qsharpCircuitLanguageId = "qsharpcircuit";
export const openqasmLanguageId = "openqasm";

// Returns true for all documents supported by the extension, including unsaved files, notebook cells, circuit files, qasm files, etc.
// excludes text documents we don't want to add support for at all, such as git/pr/chat "virtual" document views
export function isQdkDocument(document: TextDocument): boolean {
  return (
    !isUnsupportedScheme(document.uri.scheme) &&
    isQdkSupportedLanguage(document)
  );
}

function isQdkSupportedLanguage(document: TextDocument): boolean {
  return (
    document.languageId === qsharpLanguageId ||
    document.languageId === qsharpCircuitLanguageId ||
    document.languageId === openqasmLanguageId
  );
}

function isUnsupportedScheme(scheme: string): boolean {
  return (
    scheme === "git" ||
    scheme === "pr" ||
    scheme === "review" ||
    scheme.startsWith("chat")
  );
}

// Returns true for all Q# documents, including unsaved files, notebook cells, circuit files, etc.
export function isQsharpDocument(document: TextDocument): boolean {
  return (
    !isUnsupportedScheme(document.uri.scheme) &&
    document.languageId === qsharpLanguageId
  );
}

// Returns true for all circuit documents
export function isCircuitDocument(document: TextDocument): boolean {
  return (
    !isUnsupportedScheme(document.uri.scheme) &&
    document.languageId === qsharpCircuitLanguageId
  );
}

export function isQdkNotebookCell(document: TextDocument): boolean {
  return isQdkDocument(document) && isNotebookCell(document);
}

// Returns true for all OpenQASM documents, including unsaved files, notebook cells, etc.
export function isOpenQasmDocument(document: TextDocument): boolean {
  return (
    !isUnsupportedScheme(document.uri.scheme) &&
    document.languageId === openqasmLanguageId
  );
}

export function isNotebookCell(document: TextDocument): boolean {
  return document.uri.scheme === "vscode-notebook-cell";
}

export const qsharpExtensionId = "qsharp-vscode";

export function basename(path: string): string | undefined {
  return path.replace(/\/+$/, "").split("/").pop();
}

export function toVsCodeRange(range: IRange): Range {
  return new Range(
    range.start.line,
    range.start.character,
    range.end.line,
    range.end.character,
  );
}

export function toVsCodeLocation(location: ILocation): Location {
  return new Location(Uri.parse(location.source), toVsCodeRange(location.span));
}

export function toVsCodeWorkspaceEdit(
  iWorkspaceEdit: IWorkspaceEdit,
): vscode.WorkspaceEdit {
  const workspaceEdit = new vscode.WorkspaceEdit();
  for (const [source, edits] of iWorkspaceEdit.changes) {
    const uri = vscode.Uri.parse(source, true);
    const vsEdits = edits.map((edit) => {
      return new vscode.TextEdit(toVsCodeRange(edit.range), edit.newText);
    });
    workspaceEdit.set(uri, vsEdits);
  }
  return workspaceEdit;
}

export function toVsCodeDiagnostic(d: VSDiagnostic): vscode.Diagnostic {
  let severity;
  switch (d.severity) {
    case "error":
      severity = vscode.DiagnosticSeverity.Error;
      break;
    case "warning":
      severity = vscode.DiagnosticSeverity.Warning;
      break;
    case "info":
      severity = vscode.DiagnosticSeverity.Information;
      break;
  }
  const vscodeDiagnostic = new vscode.Diagnostic(
    toVsCodeRange(d.range),
    d.message,
    severity,
  );
  if (d.uri && d.code) {
    vscodeDiagnostic.code = {
      value: d.code,
      target: vscode.Uri.parse(d.uri),
    };
  } else if (d.code) {
    vscodeDiagnostic.code = d.code;
  }
  if (d.related) {
    vscodeDiagnostic.relatedInformation = d.related.map((r) => {
      return new vscode.DiagnosticRelatedInformation(
        toVsCodeLocation(r.location),
        r.message,
      );
    });
  }
  return vscodeDiagnostic;
}

export function loadCompilerWorker(extensionUri: vscode.Uri): ICompilerWorker {
  const compilerWorkerScriptPath = vscode.Uri.joinPath(
    extensionUri,
    `./out/${__PLATFORM__}/compilerWorker.js`,
  ).toString();
  return getCompilerWorker(compilerWorkerScriptPath);
}

export function getPlatformEnv(): string {
  return __PLATFORM__;
}
