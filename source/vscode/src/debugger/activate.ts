// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/* eslint-disable @typescript-eslint/no-unused-vars */

import { IDebugServiceWorker, getDebugServiceWorker, log } from "qsharp-lang";
import * as vscode from "vscode";
import { qsharpExtensionId } from "../common";
import { clearCommandDiagnostics } from "../diagnostics";
import {
  getActiveQdkDocumentUri,
  getProgramForDocument,
} from "../programConfig";
import { getRandomGuid } from "../utils";
import { QscDebugSession } from "./session";
import { runProgramInTerminal } from "../run";

let debugServiceWorkerFactory: () => IDebugServiceWorker;

export async function activateDebugger(
  context: vscode.ExtensionContext,
): Promise<void> {
  const debugWorkerScriptPath = vscode.Uri.joinPath(
    context.extensionUri,
    "./out/debugger/debug-service-worker.js",
  );

  debugServiceWorkerFactory = () =>
    getDebugServiceWorker(
      debugWorkerScriptPath.toString(),
    ) as IDebugServiceWorker;
  registerCommands(context);

  const provider = new QsDebugConfigProvider();
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider("qsharp", provider),
  );

  const factory = new InlineDebugAdapterFactory();
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory("qsharp", factory),
  );
}

function registerCommands(context: vscode.ExtensionContext) {
  // Register commands for running and debugging Q# files.
  context.subscriptions.push(
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.runProgram`,
      (resource: vscode.Uri, expr?: string) => {
        // if expr is not a string, ignore it. VS Code can sometimes
        // pass other types when this command is invoked via UI buttons.
        if (typeof expr !== "string") {
          expr = undefined;
        }
        runProgramInTerminal(
          context.extensionUri,
          resource || getActiveQdkDocumentUri(),
          "QDK: Run Program",
          expr || "",
        );
      },
    ),
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.debugProgram`,
      (resource: vscode.Uri, expr?: string) => {
        // if expr is not a string, ignore it. VS Code can sometimes
        // pass other types when this command is invoked via UI buttons.
        if (typeof expr !== "string") {
          expr = undefined;
        }
        startQdkDebugging(resource, {
          name: "QDK: Debug Program",
          stopOnEntry: true,
          entry: expr,
        });
      },
    ),
    vscode.commands.registerCommand(
      `${qsharpExtensionId}.runEditorContentsWithCircuit`,
      (resource: vscode.Uri) =>
        startQdkDebugging(
          resource,
          {
            name: "QDK: Run and Show Circuit",
            stopOnEntry: false,
            showCircuit: true,
          },
          { noDebug: true },
        ),
    ),
  );
}

export function startQdkDebugging(
  resource: vscode.Uri | undefined,
  config: { name: string; [key: string]: any },
  options?: vscode.DebugSessionOptions,
) {
  clearCommandDiagnostics();

  if (vscode.debug.activeDebugSession?.type === "qsharp") {
    // Multiple debug sessions disallowed, to reduce confusion
    return;
  }

  const targetResource = resource || getActiveQdkDocumentUri();
  if (!targetResource) {
    // No active document
    return;
  }

  if (targetResource) {
    config.programUri = targetResource.toString();

    vscode.debug.startDebugging(
      undefined,
      {
        type: "qsharp",
        request: "launch",
        shots: 1,
        ...config,
      },
      {
        // no need to save the file, in fact better not to, since it may cause the document uri to change
        suppressSaveBeforeStart: true,
        ...options,
      },
    );
  }
}

class QsDebugConfigProvider implements vscode.DebugConfigurationProvider {
  resolveDebugConfigurationWithSubstitutedVariables(
    folder: vscode.WorkspaceFolder | undefined,
    config: vscode.DebugConfiguration,
    _token?: vscode.CancellationToken | undefined,
  ): vscode.ProviderResult<vscode.DebugConfiguration> {
    if (config.program && folder) {
      // A program is specified in launch.json.
      //
      // Variable substitution is a bit odd in VS Code. Variables such as
      // ${file} and ${workspaceFolder} are expanded to absolute filesystem
      // paths with platform-specific separators. To correctly convert them
      // back to a URI, we need to use the vscode.Uri.file constructor.
      //
      // However, this gives us the URI scheme file:// , which is not correct
      // when the workspace uses a virtual filesystem such as qsharp-vfs://
      // or vscode-test-web://. So now we also need the workspace folder URI
      // to use as the basis for our file URI.
      //
      // Examples of program paths that can come through variable substitution:
      // C:\foo\bar.qs
      // \foo\bar.qs
      // /foo/bar.qs
      const fileUri = vscode.Uri.file(config.program);
      config.programUri = folder.uri
        .with({
          path: fileUri.path,
        })
        .toString();
    } else {
      // if launch.json is missing or empty, try to launch the active Q# document
      const docUri = getActiveQdkDocumentUri();
      if (docUri) {
        config.type = "qsharp";
        config.name = "Launch";
        config.request = "launch";
        config.programUri = docUri.toString();
        config.shots = 1;
        config.noDebug = "noDebug" in config ? config.noDebug : false;
        config.stopOnEntry = !config.noDebug;
        config.entry = config.entry ?? "";
      }
    }

    log.trace(
      `resolveDebugConfigurationWithSubstitutedVariables config.program=${
        config.program
      } folder.uri=${folder?.uri.toString()} config.programUri=${
        config.programUri
      }`,
    );

    if (!config.programUri) {
      // abort launch
      return vscode.window
        .showInformationMessage("Cannot find a Q# program to debug")
        .then((_) => {
          return undefined;
        });
    }
    return config;
  }

  resolveDebugConfiguration(
    folder: vscode.WorkspaceFolder | undefined,
    config: vscode.DebugConfiguration,
    _token?: vscode.CancellationToken | undefined,
  ): vscode.ProviderResult<vscode.DebugConfiguration> {
    // apply defaults if not set
    config.type = config.type ?? "qsharp";
    config.name = config.name ?? "Launch";
    config.request = config.request ?? "launch";
    config.shots = config.shots ?? 1;
    config.entry = config.entry ?? "";
    config.trace = config.trace ?? false;
    // noDebug is set to true when the user runs the program without debugging.
    // otherwise it usually isn't set, but we default to false.
    config.noDebug = config.noDebug ?? false;
    // stopOnEntry is set to true when the user runs the program with debugging.
    // unless overridden.
    config.stopOnEntry = config.stopOnEntry ?? !config.noDebug;

    return config;
  }
}

class InlineDebugAdapterFactory
  implements vscode.DebugAdapterDescriptorFactory
{
  async createDebugAdapterDescriptor(
    session: vscode.DebugSession,
    _executable: vscode.DebugAdapterExecutable | undefined,
  ): Promise<vscode.DebugAdapterDescriptor> {
    const worker = debugServiceWorkerFactory();
    const uri = vscode.Uri.parse(session.configuration.programUri);
    const file = await vscode.workspace.openTextDocument(uri);
    const program = await getProgramForDocument(file);
    if (!program.success) {
      throw new Error(program.errorMsg);
    }

    const qscSession = new QscDebugSession(
      worker,
      session.configuration,
      program.programConfig,
    );

    await qscSession.init(getRandomGuid());

    return new vscode.DebugAdapterInlineImplementation(qscSession);
  }
}
