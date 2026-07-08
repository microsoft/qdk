// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  ILanguageService,
  getLanguageService,
  loadWasmModule,
  log,
} from "qsharp-lang";
import * as vscode from "vscode";
import {
  isNotebookCell,
  isQdkDocument,
  openqasmLanguageId,
  qsharpLanguageId,
} from "../common.js";
import { getShowDevDiagnostics } from "../config.js";
import {
  fetchGithubRaw,
  findManifestDirectory,
  listDirectory,
  readFile,
  resolvePath,
} from "../projectSystem.js";
import {
  determineDocumentType,
  EventType,
  QsharpDocumentType,
  sendTelemetryEvent,
} from "../telemetry.js";
import { createCodeActionsProvider } from "./codeActions.js";
import { createQdkCodeLensProvider } from "./codeLens.js";
import { createCompletionItemProvider } from "./completion.js";
import { createDefinitionProvider } from "./definition.js";
import { startLanguageServiceDiagnostics } from "./diagnostics.js";
import { createFormattingProvider } from "./format.js";
import { createHoverProvider } from "./hover.js";
import { registerQdkNotebookCellUpdateHandlers } from "./notebook.js";
import { createReferenceProvider } from "./references.js";
import { createRenameProvider } from "./rename.js";
import { createSignatureHelpProvider } from "./signature.js";
import { startTestDiscovery } from "./testExplorer.js";

/**
 * Returns all of the subscriptions that should be registered for the language service.
 */
export async function activateLanguageService(
  context: vscode.ExtensionContext,
): Promise<vscode.Disposable[]> {
  const extensionUri = context.extensionUri;
  const subscriptions: vscode.Disposable[] = [];

  const languageService = await loadLanguageService(extensionUri);

  // diagnostics
  subscriptions.push(...startLanguageServiceDiagnostics(languageService));

  // test explorer
  subscriptions.push(...startTestDiscovery(languageService, context));

  // synchronize document contents
  subscriptions.push(...registerDocumentUpdateHandlers(languageService));

  // synchronize notebook cell contents
  subscriptions.push(...registerQdkNotebookCellUpdateHandlers(languageService));

  // synchronize configuration
  subscriptions.push(registerConfigurationChangeHandlers(languageService));

  // format document
  subscriptions.push(
    vscode.languages.registerDocumentFormattingEditProvider(
      qsharpLanguageId,
      createFormattingProvider(languageService),
    ),
  );

  // format range
  subscriptions.push(
    vscode.languages.registerDocumentRangeFormattingEditProvider(
      qsharpLanguageId,
      createFormattingProvider(languageService),
    ),
  );

  // completions
  subscriptions.push(
    vscode.languages.registerCompletionItemProvider(
      qsharpLanguageId,
      createCompletionItemProvider(languageService),
      // Trigger characters should be kept in sync with the ones in `playground/src/main.tsx`
      "@",
      ".",
    ),
  );

  subscriptions.push(
    vscode.languages.registerCompletionItemProvider(
      openqasmLanguageId,
      createCompletionItemProvider(languageService),
      "@",
      "[",
    ),
  );

  // hover
  subscriptions.push(
    vscode.languages.registerHoverProvider(
      qsharpLanguageId,
      createHoverProvider(languageService),
    ),
  );

  // go to def
  subscriptions.push(
    vscode.languages.registerDefinitionProvider(
      [qsharpLanguageId, openqasmLanguageId],
      createDefinitionProvider(languageService),
    ),
  );

  // find references
  subscriptions.push(
    vscode.languages.registerReferenceProvider(
      [qsharpLanguageId, openqasmLanguageId],
      createReferenceProvider(languageService),
    ),
  );

  // signature help
  subscriptions.push(
    vscode.languages.registerSignatureHelpProvider(
      qsharpLanguageId,
      createSignatureHelpProvider(languageService),
      "(",
      ",",
    ),
  );

  // rename symbol
  subscriptions.push(
    vscode.languages.registerRenameProvider(
      [qsharpLanguageId, openqasmLanguageId],
      createRenameProvider(languageService),
    ),
  );

  // code lens
  subscriptions.push(
    vscode.languages.registerCodeLensProvider(
      [qsharpLanguageId, openqasmLanguageId],
      createQdkCodeLensProvider(languageService),
    ),
  );

  subscriptions.push(
    vscode.languages.registerCodeActionsProvider(
      qsharpLanguageId,
      createCodeActionsProvider(languageService),
    ),
  );

  // add the language service dispose handler as well
  subscriptions.push(languageService);

  return subscriptions;
}

async function loadLanguageService(
  baseUri: vscode.Uri,
): Promise<ILanguageService> {
  const start = performance.now();
  const wasmUri = vscode.Uri.joinPath(baseUri, "./wasm/qsc_wasm_bg.wasm");
  const wasmBytes = await vscode.workspace.fs.readFile(wasmUri);
  // Cast: VS Code's readFile always returns an ArrayBuffer-backed Uint8Array,
  // but @types/vscode uses the broader ArrayBufferLike.
  await loadWasmModule(wasmBytes as Uint8Array<ArrayBuffer>);
  const languageService = await getLanguageService({
    findManifestDirectory,
    readFile,
    listDirectory,
    resolvePath: async (a, b) => resolvePath(a, b),
    fetchGithub: fetchGithubRaw,
  });
  await updateLanguageServiceConfiguration(languageService);
  const end = performance.now();
  sendTelemetryEvent(
    EventType.LoadLanguageService,
    {},
    { timeToStartMs: end - start },
  );
  return languageService;
}

/**
 * This function returns all of the subscriptions that should be registered for the language service.
 * Additionally, if an `eventEmitter` is passed in, will fire an event when a document is updated.
 */
function registerDocumentUpdateHandlers(
  languageService: ILanguageService,
): vscode.Disposable[] {
  vscode.workspace.textDocuments.forEach((document) => {
    updateIfQdkDocument(document);
  });

  // we manually send an OpenDocument telemetry event if this is a Q# document, because the
  // below subscriptions won't fire for documents that are already open when the extension is activated
  vscode.workspace.textDocuments.forEach((document) => {
    sendDocumentOpenedEvent(document);
  });

  const subscriptions = [];
  subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((document) => {
      const documentType = determineDocumentType(document);
      if (documentType !== QsharpDocumentType.Other) {
        sendTelemetryEvent(
          EventType.OpenedDocument,
          { documentType },
          { linesOfCode: document.lineCount },
        );
      }
      updateIfQdkDocument(document);
    }),
  );

  subscriptions.push(
    vscode.workspace.onDidChangeTextDocument((evt) => {
      updateIfQdkDocument(evt.document);
    }),
  );

  subscriptions.push(
    vscode.workspace.onDidCloseTextDocument((document) => {
      if (isQdkDocument(document) && !isNotebookCell(document)) {
        languageService.closeDocument(
          document.uri.toString(),
          document.languageId,
        );
      }
    }),
  );

  // Watch manifest changes and update each document in the same project as the manifest.
  subscriptions.push(
    vscode.workspace.onDidSaveTextDocument((manifest) => {
      updateProjectDocuments(manifest.uri);
    }),
  );

  // Trigger an update on all .qs child documents when their manifest is deleted,
  // so that they can get reparented to single-file-projects.
  subscriptions.push(
    vscode.workspace.onDidDeleteFiles((event) => {
      event.files.forEach((uri) => {
        updateProjectDocuments(uri);
      });
    }),
  );

  // Checks if the URI belongs to a qsharp manifest, and updates all
  // open documents in the same project as the manifest.
  function updateProjectDocuments(manifest: vscode.Uri) {
    if (manifest.scheme === "file" && manifest.fsPath.endsWith("qsharp.json")) {
      const project_folder = manifest.fsPath.slice(
        0,
        manifest.fsPath.length - "qsharp.json".length,
      );
      vscode.workspace.textDocuments.forEach((document) => {
        if (
          !document.isClosed &&
          // Check that the document is on the same project as the manifest.
          document.fileName.startsWith(project_folder)
        ) {
          updateIfQdkDocument(document);
        }
      });
    }
  }

  async function updateIfQdkDocument(document: vscode.TextDocument) {
    if (isQdkDocument(document) && !isNotebookCell(document)) {
      const content = document.getText();

      languageService.updateDocument(
        document.uri.toString(),
        document.version,
        content,
        document.languageId,
      );
    }
  }

  return subscriptions;
}

function sendDocumentOpenedEvent(document: vscode.TextDocument) {
  if (isQdkDocument(document)) {
    const documentType = determineDocumentType(document);
    sendTelemetryEvent(
      EventType.OpenedDocument,
      { documentType },
      { linesOfCode: document.lineCount },
    );
  }
}

function registerConfigurationChangeHandlers(
  languageService: ILanguageService,
) {
  return vscode.workspace.onDidChangeConfiguration((event) => {
    if (event.affectsConfiguration("Q#.dev.showDevDiagnostics")) {
      updateLanguageServiceConfiguration(languageService);
    }
  });
}

async function updateLanguageServiceConfiguration(
  languageService: ILanguageService,
) {
  const showDevDiagnostics = getShowDevDiagnostics();

  log.debug("Show dev diagnostics set to: " + showDevDiagnostics);

  // Update all configuration settings
  languageService.updateConfiguration({
    devDiagnostics: showDevDiagnostics,
    lints: [{ lint: "needlessOperation", level: "warn" }],
  });
}
