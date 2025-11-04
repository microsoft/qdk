// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { IQSharpError } from "qsharp-lang";
import vscode from "vscode";
import { loadCompilerWorker } from "./common";
import { createDebugConsoleEventTarget } from "./debugger/output";
import { FullProgramConfig, getProgramForDocument } from "./programConfig";
import { clearCommandDiagnostics } from "./diagnostics";

/**
 * Runs a program in a VS Code terminal using a custom pseudoterminal.
 *
 * @param extensionUri - The URI of the extension, used for resource paths.
 * @param document - The URI of the document to run.
 * @param terminalName - The name to display for the terminal.
 * @param entry - The entry point for the program execution.
 */
export function runProgramInTerminal(
  extensionUri: vscode.Uri,
  document: vscode.Uri,
  terminalName: string,
  entry: string,
) {
  clearCommandDiagnostics();

  if (document) {
    const cancellationTokenSource = new vscode.CancellationTokenSource();

    const output = new vscode.EventEmitter<string>();
    const closeEmitter = new vscode.EventEmitter<void>();
    const pty: vscode.Pseudoterminal = {
      onDidWrite: output.event,
      onDidClose: closeEmitter.event,
      open: async () => {
        const programUri = document.toString();
        const uri = vscode.Uri.parse(programUri);
        const file = await vscode.workspace.openTextDocument(uri);

        const program = await getProgramForDocument(file);
        if (!program.success) {
          throw new Error(program.errorMsg);
        }

        const result = await runProgram(extensionUri, program.programConfig, {
          entry,
          shots: 1,
          onConsoleOut: (msg) => {
            // replace \n with \r\n for proper terminal display
            msg = msg.replace(/\n/g, "\r\n");
            output.fire(msg + "\r\n");
          },
          cancellationToken: cancellationTokenSource.token,
        });
        if (result.status !== ProgramRunStatus.AllShotsDone) {
          output.fire(`\r\nProgram terminated due to ${result.status}.\r\n`);
        }
        output.fire("\r\nPress any key to close this terminal.\r\n");
      },
      close: () => {
        cancellationTokenSource.cancel();
      },
      handleInput: () => {
        // Any key press closes the terminal after program completion
        closeEmitter.fire();
      },
    };

    const terminal = vscode.window.createTerminal({
      name: terminalName,
      pty,
      iconPath: {
        light: vscode.Uri.joinPath(
          extensionUri,
          "resources",
          "file-icon-light.svg",
        ),
        dark: vscode.Uri.joinPath(
          extensionUri,
          "resources",
          "file-icon-dark.svg",
        ),
      },
      isTransient: true,
    });

    terminal.show();
  }
}

const enum ProgramRunStatus {
  AllShotsDone,
  Timeout,
  Cancellation,
  CompilationErrors,
}

// More strongly typed than the `qsharp-lang` equivalent
type ShotResult =
  | {
      success: true;
      result: string;
    }
  | {
      success: false;
      errors: IQSharpError[];
    }[];

type ProgramRunResult =
  | {
      // Some or all shots were executed.
      status:
        | ProgramRunStatus.AllShotsDone
        | ProgramRunStatus.Timeout
        | ProgramRunStatus.Cancellation;
      // Results for each shot executed (0 or more).
      shotResults: ShotResult[];
    }
  | {
      // No shots were executed due to compilation errors.
      status: "compilation error(s)";
      errors: IQSharpError[];
    };

/**
 * Histogram data for displaying results
 */
export type HistogramData = {
  buckets: [string, number][];
  shotCount?: number;
};
