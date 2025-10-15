// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  getLibrarySourceContent,
  ILocation,
  log,
  qsharpGithubUriScheme,
  qsharpLibraryUriScheme,
} from "qsharp-lang";
import * as vscode from "vscode";
import { initAzureWorkspaces } from "./azure/commands.js";
import { CircuitEditorProvider } from "./circuitEditor.js";
import { initProjectCreator } from "./createProject.js";
import { activateDebugger } from "./debugger/activate.js";
import { startOtherQSharpDiagnostics } from "./diagnostics.js";
import { registerGhCopilotInstructionsCommand } from "./gh-copilot/instructions.js";
import { registerLanguageModelTools } from "./gh-copilot/tools.js";
import { activateLanguageService } from "./language-service/activate.js";
import {
  Logging,
  initLogForwarder,
  initOutputWindowLogger,
} from "./logging.js";
import { initFileSystem } from "./memfs.js";
import {
  registerCreateNotebookCommand,
  registerQSharpNotebookHandlers,
} from "./notebook.js";
import { getGithubSourceContent, setGithubEndpoint } from "./projectSystem.js";
import { initCodegen } from "./qirGeneration.js";
import { initTelemetry } from "./telemetry.js";
import { registerWebViewCommands } from "./webviewPanel.js";
import {
  maybeShowChangelogPrompt,
  registerChangelogCommand,
} from "./changelog.js";
import { toVsCodeRange } from "./common.js";

export async function activate(
  context: vscode.ExtensionContext,
): Promise<ExtensionApi> {
  const api: ExtensionApi = { setGithubEndpoint };

  if (context.extensionMode === vscode.ExtensionMode.Test) {
    // Don't log to the output window in tests, forward to a listener instead
    api.logging = initLogForwarder();
  } else {
    // Direct logging to the output window
    initOutputWindowLogger();
  }

  log.info("Q# extension activating.");
  initTelemetry(context);

  checkForOldQdk();

  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(
      qsharpLibraryUriScheme,
      new QsTextDocumentContentProvider(),
    ),
  );

  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(
      qsharpGithubUriScheme,
      {
        provideTextDocumentContent(uri) {
          return getGithubSourceContent(uri);
        },
      },
    ),
  );

  context.subscriptions.push(...(await activateLanguageService(context)));
  context.subscriptions.push(...startOtherQSharpDiagnostics());
  context.subscriptions.push(...registerQSharpNotebookHandlers());
  context.subscriptions.push(CircuitEditorProvider.register(context));
  context.subscriptions.push(...registerChangelogCommand(context));

  await initAzureWorkspaces(context);
  initCodegen(context);
  await activateDebugger(context);
  registerCreateNotebookCommand(context);
  registerWebViewCommands(context);
  await initFileSystem(context);
  await initProjectCreator(context);
  registerLanguageModelTools(context);
  // fire-and-forget
  registerGhCopilotInstructionsCommand(context);

  // Show prompt after update if not suppressed
  maybeShowChangelogPrompt(context);

  context.subscriptions.push(
    vscode.commands.registerCommand(
      "qsharp-vscode.gotoLocations",
      async (locations: ILocation[]) => {
        // location sources can be a proper URI, or a compiler-generated pseudo-source
        // file like `<entry>`. We only want to navigate if it's a proper URI.

        log.debug(`gotoLocation`, locations);
        const validLocations = parseLocations(locations);

        if (validLocations.length === 0) {
          log.debug("No valid locations to navigate to.");
          return;
        }

        for (const l of validLocations) {
          const document = await vscode.workspace.openTextDocument(l.uri);
          const existingEditor = vscode.window.visibleTextEditors.find(
            (e) => e.document.uri.toString() === l.uri.toString(),
          );
          const viewColumn =
            existingEditor?.viewColumn ?? vscode.ViewColumn.One;

          // Force the document to open and take focus first in our preferred
          // view column - this prevents the `goToLocations` command
          // below from opening the document in whatever view column currently
          // has focus.
          await vscode.window.showTextDocument(document, {
            viewColumn,
          });
          // editor.revealRange(vscodeRange, vscode.TextEditorRevealType.InCenter);
        }

        vscode.commands.executeCommand(
          "editor.action.goToLocations",
          validLocations[0].uri,
          validLocations[0].range.start,
          validLocations,
          "peek",
        );
      },
    ),
  );

  log.info("Q# extension activated.");

  return api;
}

function parseLocations(locations: ILocation[]): vscode.Location[] {
  const uris = [];
  for (const loc of locations) {
    try {
      const uri = vscode.Uri.parse(loc.source);
      uris.push(new vscode.Location(uri, toVsCodeRange(loc.span)));
    } catch {
      // ignore invalid URIs
      log.debug(`Ignoring invalid URI: ${loc.source}`);
      continue;
    }
  }
  return uris;
}

export interface ExtensionApi {
  // Only available in test mode. Allows listening to extension log events.
  logging?: Logging;
  setGithubEndpoint: (endpoint: string) => void;
}

export class QsTextDocumentContentProvider
  implements vscode.TextDocumentContentProvider
{
  onDidChange?: vscode.Event<vscode.Uri> | undefined;
  provideTextDocumentContent(uri: vscode.Uri): vscode.ProviderResult<string> {
    return getLibrarySourceContent(uri.toString());
  }
}

function checkForOldQdk() {
  const oldQdkExtension = vscode.extensions.getExtension(
    "quantum.quantum-devkit-vscode",
  );

  const prereleaseQdkExtension = vscode.extensions.getExtension(
    "quantum.qsharp-lang-vscode-dev",
  );

  const releaseQdkExtension = vscode.extensions.getExtension(
    "quantum.qsharp-lang-vscode",
  );

  const previousQdkWarningMessage =
    'Extension "Microsoft Quantum Development Kit for Visual Studio" (`quantum.quantum-devkit-vscode`) found. We recommend uninstalling the prior QDK before using this release.';

  const bothReleaseAndPrereleaseWarningMessage =
    'Extension "Azure Quantum Development Kit (QDK)" has both release and pre-release versions installed. We recommend uninstalling one of these versions.';

  // we don't await the warnings below so we don't block extension initialization
  if (oldQdkExtension) {
    log.warn(previousQdkWarningMessage);
    vscode.window.showWarningMessage(previousQdkWarningMessage);
  }

  if (prereleaseQdkExtension && releaseQdkExtension) {
    log.warn(bothReleaseAndPrereleaseWarningMessage);
    vscode.window.showWarningMessage(bothReleaseAndPrereleaseWarningMessage);
  }
}
