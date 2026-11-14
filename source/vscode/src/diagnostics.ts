// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log, QdkDiagnostics, IQSharpError } from "qsharp-lang";
import * as vscode from "vscode";
import {
  qsharpExtensionId,
  qsharpLanguageId,
  toVsCodeDiagnostic,
} from "./common.js";
import { getSourceUri } from "./utils.js";

/**
 * Initialize diagnostics for `qsharp.json` files and failures
 * that get reported from various Q# commands.
 *
 * These are distinct from the errors reported by the Q# language
 * service, (a.k.a. compiler errors that get reported as you type).
 * Those are initialized in `language-service/diagnostics.js`
 */
export function startOtherQSharpDiagnostics(): vscode.Disposable[] {
  return [...startQsharpJsonDiagnostics(), ...startCommandDiagnostics()];
}

//
// qsharp.json diagnostics.
//
// These are reported whenever problems are detected in a qsharp.json file.
//

let qsharpJsonDiagnostics: vscode.DiagnosticCollection | undefined;
let deleteQsharpJsonListener: vscode.Disposable | undefined;
const trackedQsharpJsons = new Set<string>();

function startQsharpJsonDiagnostics(): vscode.Disposable[] {
  qsharpJsonDiagnostics =
    vscode.languages.createDiagnosticCollection(qsharpLanguageId);

  deleteQsharpJsonListener = vscode.workspace.onDidDeleteFiles((event) => {
    for (const uri of event.files) {
      if (trackedQsharpJsons.delete(uri.toString())) {
        // Clear the diagnostics when the qsharp.json is deleted
        qsharpJsonDiagnostics?.set(uri, []);
      }
    }
  });

  return [qsharpJsonDiagnostics, deleteQsharpJsonListener];
}

export function updateQSharpJsonDiagnostics(
  qsharpJson: vscode.Uri,
  error?: string,
) {
  if (!qsharpJsonDiagnostics) {
    log.warn("no diagnostic collection for qsharp.json, not reporting");
  }

  trackedQsharpJsons.add(qsharpJson.toString());

  const errors = error
    ? [
        new vscode.Diagnostic(
          new vscode.Range(0, 0, 0, 0),
          error,
          vscode.DiagnosticSeverity.Error,
        ),
      ]
    : [];

  qsharpJsonDiagnostics?.set(qsharpJson, errors);
}

//
// Command diagnostics.
//
// These are Q# diagnostics that are caused by the invocation of a command, e.g. "generate QIR".
//
// These behave differently than diagnostics coming from the language service. They are not live.
// They pertain to the last-run command. They don't get cleared or refreshed when the
// user edits the document. They will be cleared when a new command is run.
// They can also be dismissed by the user with a code action.
//

let commandDiagnostics: vscode.DiagnosticCollection | undefined;

function startCommandDiagnostics(): vscode.Disposable[] {
  commandDiagnostics =
    vscode.languages.createDiagnosticCollection(qsharpLanguageId);

  const dismissCommand = vscode.commands.registerCommand(
    `${qsharpExtensionId}.dismissCommandDiagnostics`,
    clearCommandDiagnostics,
  );

  const dismissActionProvider = vscode.languages.registerCodeActionsProvider(
    [qsharpLanguageId, { language: "json", pattern: "**/qsharp.json" }],
    {
      provideCodeActions(doc, range, context): vscode.CodeAction[] | undefined {
        const commandErrors = context.diagnostics.filter(
          (d) =>
            d.message.startsWith("QDK command error: ") ||
            d.message.startsWith("QDK command warning: "),
        );

        if (commandErrors.length > 0) {
          const action = new vscode.CodeAction(
            "Dismiss diagnostics for the last run QDK command",
            vscode.CodeActionKind.QuickFix, // makes the lightbulb reliably show up
          );
          action.diagnostics = commandErrors;
          action.command = {
            command: `${qsharpExtensionId}.dismissCommandDiagnostics`,
            title: "Dismiss errors for the last run QDK command",
          };
          action.isPreferred = true;
          return [action];
        }
      },
    },
  );

  return [commandDiagnostics, dismissCommand, dismissActionProvider];
}

export function clearCommandDiagnostics() {
  if (!commandDiagnostics) {
    log.warn(`no diagnostic collection for commands, not clearing`);
  }

  commandDiagnostics?.clear();
}

export function reportCommandDiagnostics(diagnostics: IQSharpError[]) {
  const byUri = new Map<string, vscode.Diagnostic[]>();
  for (const error of diagnostics) {
    const uri = getSourceUri(error.document).toString();
    const label = error.diagnostic.severity === "warning" ? "warning" : "error";
    const diagnostic = toVsCodeDiagnostic({
      ...error.diagnostic,
      message: `QDK command ${label}: ${error.diagnostic.message}`,
    });
    const group = byUri.get(uri) ?? [];
    group.push(diagnostic);
    byUri.set(uri, group);
  }
  for (const [uri, group] of byUri) {
    commandDiagnostics?.set(vscode.Uri.parse(uri), group);
  }
}

/**
 * Wrapper that invokes a function. This function would be something that calls into
 * the Q# compiler. If the function throws an exception that contains Q# diagnostics,
 * reports the diagnostics to VS Code before rethrowing the original exception.
 */
export async function invokeAndReportCommandDiagnostics<T>(
  fn: () => T | Promise<T>,
  options: {
    showModalError?: boolean;
    populateProblemsView?: boolean;
  },
): Promise<T> {
  try {
    // Clear the diagnostics from the last command run.
    clearCommandDiagnostics();
    return await fn();
  } catch (e: unknown) {
    reportIfQSharpErrors(e, options);
    throw e;
  }
}

/**
 * Given an exception, checks if it's an instance of `QdkDiagnostics`,
 * and if so, reports the diagnostics to VS Code.
 *
 * @param e an exception originating from the qsharp-lang package
 */
function reportIfQSharpErrors(
  e: unknown,
  options: {
    showModalError?: boolean;
    populateProblemsView?: boolean;
  },
) {
  if (!commandDiagnostics) {
    log.warn(`diagnostic collection for commands was not initialized`);
    return;
  }

  if (e instanceof QdkDiagnostics) {
    if (options.populateProblemsView) {
      reportCommandDiagnostics(e.diagnostics);
    }

    if (options.showModalError) {
      // fire and forget
      showModalError(e.message);
    }
  }
}

async function showModalError(errorMsg: string) {
  const choice = await vscode.window.showErrorMessage(
    "The command returned errors. Please see the Problems view.\n\n" + errorMsg,
    { modal: true },
    {
      title: "Show Problems",
      isCloseAffordance: true,
    },
  );
  await vscode.commands.executeCommand(
    "workbench.action.problems.focus",
    choice,
  );
}
