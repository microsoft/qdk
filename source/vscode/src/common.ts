// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
declare const __PLATFORM__: "browser" | "node";

import { escapeHtml } from "markdown-it/lib/common/utils.mjs";
import { TextDocument, Uri, Range, Location } from "vscode";
import {
  getCompilerWorker,
  ICompilerWorker,
  ILocation,
  IPosition,
  IQSharpError,
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

/**
 * Formats an array of compiler/runtime errors into HTML to be presented to the user.
 *
 * @param errors The list of errors to format.
 * @returns The HTML formatted errors, to be set as the inner contents of a container element.
 */
export function errorsToHtml(errors: IQSharpError[]) {
  let errorHtml = "";
  for (const error of errors) {
    const { document, diagnostic: diag, stack: rawStack } = error;

    const location = documentHtml(false, document, diag.range.start);
    const message = escapeHtml(`(${diag.code}) ${diag.message}`).replace(
      /\n/g,
      "<br/><br/>",
    );

    errorHtml += `<p>${location}: ${message}<br/></p>`;

    if (rawStack) {
      const stack = rawStack
        .split("\n")
        .map((l) => {
          // Link-ify the document names in the stack trace
          const match = l.match(/^(\s*)at (.*) in (.*):(\d+):(\d+)/);
          if (match) {
            const [, leadingWs, callable, doc] = match;
            return `${leadingWs}at ${escapeHtml(callable)} in ${documentHtml(false, doc)}`;
          } else {
            return l;
          }
        })

        .join("\n");
      errorHtml += `<br/><pre>${stack}</pre>`;
    }
  }
  return errorHtml;
}

/**
 * If the input is a URI, turns it into a document open link.
 * Otherwise returns the HTML-escaped input
 */
function documentHtml(
  customCommand: boolean,
  maybeUri: string,
  position?: IPosition,
) {
  try {
    // If the error location is a document URI, create a link to that document.
    // We use the `vscode.open` command (https://code.visualstudio.com/api/references/commands#commands)
    // to open the document in the editor.
    // The line and column information is displayed, but are not part of the link.
    //
    // At the time of writing this is the only way we know to create a direct
    // link to a Q# document from a Web View.
    //
    // If we wanted to handle line/column information from the link, an alternate
    // implementation might be having our own command that navigates to the correct
    // location. Then this would be a link to that command instead. Yet another
    // alternative is to have the webview pass a message back to the extension.
    const uri = Uri.parse(maybeUri, true);
    const fsPath = escapeHtml(basename(uri.path) ?? uri.fsPath);
    const lineColumn = position
      ? escapeHtml(`:${position.line + 1}:${position.character + 1}`)
      : "";

    const locations = [
      {
        source: uri,
        span: {
          start: position,
          end: position,
        },
      },
    ];

    const args = customCommand && position ? [locations] : [uri];
    const openCommand =
      customCommand && position ? "qsharp-vscode.gotoLocations" : "vscode.open";

    const argsStr = encodeURIComponent(JSON.stringify(args));
    const openCommandUri = Uri.parse(`command:${openCommand}?${argsStr}`, true);
    const title = `${fsPath}${lineColumn}`;
    return `<a href="${openCommandUri}">${title}</a>`;
  } catch {
    // Likely could not parse document URI - it must be a project level error
    // or an error from stdlib, use the document name directly
    return escapeHtml(maybeUri);
  }
}
