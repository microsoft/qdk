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
 * Executes a Q# program using a compiler worker, collects results, and streams output.
 *
 * @param extensionUri - The URI of the extension, used for resource paths.
 * @param program - The full program configuration to run.
 * @returns A promise that resolves with the final list of results.
 */
export async function runProgram(
  extensionUri: vscode.Uri,
  program: FullProgramConfig,
  options: {
    /**
     * The entry expression (if omitted, the entrypoint from the program used).
     */
    entry?: string;
    /**
     * The number of shots to run (if omitted, defaults to 1).
     */
    shots?: number;
    /**
     * Callback for console output.
     */
    onConsoleOut?: (message: string) => void;
    /**
     * Callback for program results (histogram updates and failures).
     */
    onResultsUpdate?: (
      histogram: HistogramData,
      failures: IQSharpError[],
    ) => void;
    /**
     * A cancellation token to cancel the run.
     */
    cancellationToken?: vscode.CancellationToken;
  },
): Promise<ProgramRunResult> {
  let histogram: HistogramData | undefined;
  const evtTarget = createDebugConsoleEventTarget((msg) => {
    options.onConsoleOut?.(msg);
  }, true /* captureEvents */);

  // create a promise that we'll resolve when the run is done
  const allShotsDone = new Promise<void>((resolve) => {
    evtTarget.addEventListener("uiResultsRefresh", () => {
      const results = evtTarget.getResults();
      const resultCount = evtTarget.resultCount(); // compiler errors come through here too
      const buckets = new Map();
      const failures = [];
      for (let i = 0; i < resultCount; ++i) {
        const key = results[i].result;
        const strKey = typeof key !== "string" ? "ERROR" : key;
        const newValue = (buckets.get(strKey) || 0) + 1;
        buckets.set(strKey, newValue);
        if (!results[i].success) {
          const errors = (results[i].result as { errors: IQSharpError[] })
            .errors;
          failures.push(...errors);
        }
      }
      histogram = {
        buckets: Array.from(buckets.entries()) as [string, number][],
        shotCount: resultCount,
      };
      options.onResultsUpdate?.(histogram, failures);
      if (
        options.shots === resultCount ||
        failures.length > 0 ||
        options.cancellationToken?.isCancellationRequested
      ) {
        // TODO: ugh
        resolve();
      }
    });
  });

  let doneReason = ProgramRunStatus.AllShotsDone;
  const compilerRunTimeoutMs = 1000 * 60 * 5; // 5 minutes
  const compilerTimeout = setTimeout(() => {
    doneReason = ProgramRunStatus.Timeout;
    worker.terminate();
  }, compilerRunTimeoutMs);
  options.cancellationToken?.onCancellationRequested(() => {
    doneReason = ProgramRunStatus.Cancellation;
    worker.terminate();
  });
  // Final check before long running operation
  if (options.cancellationToken?.isCancellationRequested) {
    doneReason = ProgramRunStatus.Cancellation;
    return { status: doneReason, shotResults: [] };
  }

  const worker = loadCompilerWorker(extensionUri!);

  try {
    await worker.run(
      program,
      options.entry || "",
      options.shots || 1,
      evtTarget,
    );
    // We can still receive events after the above call is done.
    // Await until all shots are complete.
    await allShotsDone;
  } catch {
    // Compiler errors can come through here. But the error object here doesn't contain enough
    // information to be useful. So wait for the one that comes through the event target.
    await allShotsDone;

    doneReason = ProgramRunStatus.CompilationErrors;
    const errors = evtTarget
      .getResults()
      .flatMap((r) =>
        !r.success && r.result && typeof r.result !== "string"
          ? r.result.errors
          : [],
      );
    return { status: "compilation error(s)", errors };
  }
  clearTimeout(compilerTimeout);
  worker.terminate();
  return {
    shotResults: evtTarget.getResults().map(
      (r) =>
        (r.success
          ? {
              success: true,
              result: r.result as string,
            }
          : {
              success: false,
              errors: (r.result as { errors: IQSharpError[] }).errors,
            }) as ShotResult,
    ),
    status: doneReason,
  };
}

/**
 * Histogram data for displaying results
 */
export type HistogramData = {
  buckets: [string, number][];
  shotCount?: number;
};
