// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  getLibrarySourceContent,
  log,
  qsharpGithubUriScheme,
  qsharpLibraryUriScheme,
} from "qsharp-lang";
import * as vscode from "vscode";
import { initAzureWorkspaces } from "./azure/commands.js";
import { initProjectCreator } from "./createProject.js";
import { activateDebugger } from "./debugger/activate.js";
import { startOtherQSharpDiagnostics } from "./diagnostics.js";
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
import { activateTargetProfileStatusBarItem } from "./statusbar.js";
import { initTelemetry } from "./telemetry.js";
import { registerWebViewCommands } from "./webviewPanel.js";

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

  context.subscriptions.push(...activateTargetProfileStatusBarItem());

  context.subscriptions.push(
    ...(await activateLanguageService(context.extensionUri)),
  );

  context.subscriptions.push(...startOtherQSharpDiagnostics());

  context.subscriptions.push(...registerQSharpNotebookHandlers());

  initAzureWorkspaces(context);
  initCodegen(context);
  activateDebugger(context);
  registerCreateNotebookCommand(context);
  registerWebViewCommands(context);
  initFileSystem(context);
  initProjectCreator(context);

  log.info("Q# extension activated.");

  return api;
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
