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

/** How long after the last edit to wait before refreshing squiggles. */
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

    publisher.onDiagnosticsUpdate(diagnostics.uri, diagnostics.diagnostics);
  }

  languageService.addEventListener("diagnostics", onDiagnostics);

  // Feed edit events to the DiagnosticsPublisher
  const diagnosticsPublisherEditTracker =
    vscode.workspace.onDidChangeTextDocument((evt) => {
      // Dirty-state and encoding changes raise this event too, with no edit behind them.
      if (evt.contentChanges.length === 0) {
        return;
      }

      publisher.onEdit(
        isQdkDocument(evt.document) ? evt.document.uri.toString() : undefined,
      );
    });

  return [
    {
      dispose: () => {
        languageService.removeEventListener("diagnostics", onDiagnostics);
        publisher.dispose();
      },
    },
    diagnosticsPublisherEditTracker,
    diagCollection,
  ];
}
