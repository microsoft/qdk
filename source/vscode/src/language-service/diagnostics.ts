// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  DiagnosticsPublisher,
  ILanguageService,
  VSDiagnostic,
  qsharpLibraryUriScheme,
} from "qsharp-lang";
import * as vscode from "vscode";
import { isQdkDocument, qsharpLanguageId, toVsCodeDiagnostic } from "../common";

/** How long after the last keystroke to wait before refreshing squiggles. */
const idleDelayMs = 300;

/** Upper bound on how long sustained typing can withhold a refresh. */
const maxDelayMs = 1500;

export function startLanguageServiceDiagnostics(
  languageService: ILanguageService,
): vscode.Disposable[] {
  const diagCollection =
    vscode.languages.createDiagnosticCollection(qsharpLanguageId);

  const publisher = new DiagnosticsPublisher({
    publish: (uri, diagnostics) =>
      diagCollection.set(
        vscode.Uri.parse(uri),
        diagnostics.map((d) => toVsCodeDiagnostic(d)),
      ),
    schedule: (callback, delayMs) => {
      const handle = setTimeout(callback, delayMs);
      return () => clearTimeout(handle);
    },
    delayMs: idleDelayMs,
    maxDelayMs,
  });

  async function onDiagnostics(evt: {
    detail: {
      uri: string;
      version: number;
      diagnostics: VSDiagnostic[];
    };
  }) {
    const diagnostics = evt.detail;
    const uri = vscode.Uri.parse(diagnostics.uri);

    if (uri.scheme === qsharpLibraryUriScheme) {
      // Don't report diagnostics for library files.
      return;
    }

    publisher.receive(diagnostics.uri, diagnostics.diagnostics);
  }

  languageService.addEventListener("diagnostics", onDiagnostics);

  // A change event, rather than the active editor, is what marks a document as being typed in.
  // Documents the language service never publishes for are ignored rather than clearing the hot
  // document, since adopting one would silently disable the debounce until the user typed in a
  // QDK file again.
  const hotDocumentTracker = vscode.workspace.onDidChangeTextDocument((evt) => {
    if (isQdkDocument(evt.document)) {
      publisher.setHotUri(evt.document.uri.toString());
    }
  });

  return [
    {
      dispose: () => {
        languageService.removeEventListener("diagnostics", onDiagnostics);
        publisher.dispose();
      },
    },
    hotDocumentTracker,
    diagCollection,
  ];
}
