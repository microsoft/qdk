// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { IQSharpError, ShotResult } from "qsharp-lang";
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

        const result = await runProgram(
          extensionUri,
          program.programConfig,
          entry,
          1,
          (msg) => {
            // replace \n with \r\n for proper terminal display
            msg = msg.replace(/\n/g, "\r\n");
            output.fire(msg + "\r\n");
          },
          () => {},
          cancellationTokenSource.token,
        );
        if (result.doneReason !== "all shots done") {
          output.fire(
            `\r\nProgram terminated due to ${result.doneReason}.\r\n`,
          );
        }
        output.fire("\r\nPress any key to close this terminal.\r\n");
      },
      close: () => {
        // TODO: cleanup
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

/**
 * Executes a Q# program using a compiler worker, collects results, and streams output.
 *
 * @param extensionUri - The URI of the extension, used for resource paths.
 * @param program - The full program configuration to run.
 * @param entry - The entry point for the program; an empty string indicates the default entry point.
 * @param shots - The number of times to run the program.
 * @param onOutput - Callback for output messages.
 * @param onResults - Progress callback for updating result histograms and reporting failures.
 * @returns A promise that resolves with the final list of results.
 */
export async function runProgram(
  extensionUri: vscode.Uri,
  program: FullProgramConfig,
  entry: string, // can be "" to indicate default entrypoint
  shots: number,
  onOutput: (message: string) => void,
  onResults: (histogram: HistogramData, failures: IQSharpError[]) => void,
  cancellationToken?: vscode.CancellationToken,
): Promise<{
  results: ShotResult[];
  doneReason:
    | "all shots done"
    | "compilation error(s)"
    | "timeout"
    | "cancellation";
}> {
  let histogram: HistogramData | undefined;
  const evtTarget = createDebugConsoleEventTarget((msg) => {
    onOutput(msg);
  }, true /* captureEvents */);

  // create a promise that we'll resolve when the run is done
  let resolvePromise: () => void = () => {};
  const allShotsDone = new Promise<void>((resolve) => {
    resolvePromise = resolve;
  });

  evtTarget.addEventListener("uiResultsRefresh", () => {
    const results = evtTarget.getResults();
    const resultCount = evtTarget.resultCount(); // compiler errors come through here too
    const buckets = new Map();
    const failures = [];
    for (let i = 0; i < resultCount; ++i) {
      const result = results[i];
      const key = result.result;
      const strKey = typeof key !== "string" ? "ERROR" : key;
      const newValue = (buckets.get(strKey) || 0) + 1;
      buckets.set(strKey, newValue);
      if (!result.success) {
        const errors = (result.result as { errors: IQSharpError[] }).errors;
        failures.push(...errors);
      }
    }
    histogram = {
      buckets: Array.from(buckets.entries()) as [string, number][],
      shotCount: resultCount,
    };
    onResults(histogram!, failures);
    if (shots === resultCount || failures.length > 0) {
      resolvePromise();
    }
  });

  let doneReason:
    | "all shots done"
    | "timeout"
    | "cancellation"
    | "compilation error(s)" = "all shots done";
  const compilerRunTimeoutMs = 1000 * 60 * 5; // 5 minutes
  const compilerTimeout = setTimeout(() => {
    doneReason = "timeout";
    worker.terminate();
  }, compilerRunTimeoutMs);
  cancellationToken?.onCancellationRequested(() => {
    doneReason = "cancellation";
    worker.terminate();
  });
  const worker = loadCompilerWorker(extensionUri!);

  try {
    await worker.run(program, entry, shots, evtTarget);
    // We can still receive events after the above call is done
    await allShotsDone;
  } catch {
    // Compiler errors can come through here. But the error object here doesn't contain enough
    // information to be useful. So wait for the one that comes through the event target.
    await allShotsDone;

    doneReason = "compilation error(s)";
  }
  clearTimeout(compilerTimeout);
  worker.terminate();
  return { results: evtTarget.getResults(), doneReason };
}

/**
 * Histogram data for displaying results
 */
export type HistogramData = {
  buckets: [string, number][];
  shotCount?: number;
};
