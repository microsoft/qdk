// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { IQSharpError, log } from "qsharp-lang";
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
    let done = false;
    const cancellationTokenSource = new vscode.CancellationTokenSource();
    const output = new vscode.EventEmitter<string>();
    const closeEmitter = new vscode.EventEmitter<void>();
    const pty: vscode.Pseudoterminal = {
      onDidWrite: output.event,
      onDidClose: closeEmitter.event,
      open: async () => {
        await runOnTerminalOpen(
          document,
          extensionUri,
          entry,
          output,
          cancellationTokenSource.token,
        );
        done = true;
      },
      close: () => {
        log.debug("Terminal closed, cancelling program run.");
        cancellationTokenSource.cancel();
      },
      handleInput: (data) => {
        // Any key press closes the terminal after program completion
        if (done) {
          closeEmitter.fire();
        } else if (data === "\x03") {
          // ETX / Ctrl+C
          cancellationTokenSource.cancel();
        }
        log.debug("Ignoring terminal input.");
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
  AllShotsDone = "all shots done",
  Timeout = "timeout",
  Cancellation = "cancellation",
  CompilationErrors = "compilation error(s)",
  FatalError = "fatal error",
  UnknownError = "unknown error",
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
        | ProgramRunStatus.Cancellation
        | ProgramRunStatus.Timeout
        | ProgramRunStatus.FatalError
        | ProgramRunStatus.UnknownError;
      // Results for each shot executed (0 or more).
      shotResults: ShotResult[];
    }
  | {
      // No shots were executed due to compilation errors.
      status: ProgramRunStatus.CompilationErrors;
      errors: IQSharpError[];
    };

/**
 * Histogram data for displaying results
 */
export type HistogramData = {
  buckets: [string, number][];
  shotCount?: number;
};

/**
 * Executes a Q# program using a compiler worker, collects results, and streams output.
 *
 * @param extensionUri - The URI of the extension, used for resource paths.
 * @param program - The full program configuration to run.
 * @returns A promise that resolves with the final list of results.
 */
export function runProgram(
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
  return new Promise<ProgramRunResult>(function executeRunProgram(
    resolve,
  ): void {
    let histogram: HistogramData | undefined;
    const evtTarget = createDebugConsoleEventTarget((msg) => {
      options.onConsoleOut?.(msg);
    }, true /* captureEvents */);

    evtTarget.addEventListener("uiResultsRefresh", () => {
      const results = evtTarget.getResults();
      const resultCount = evtTarget.resultCount(); // compiler errors come through here too
      const buckets = new Map();
      const failures: IQSharpError[] = [];
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

      // Somewhat hacky way of determining when we are done and
      // don't expect to receive any more results.
      // Ideally the `evtTarget` would contain a definitive "all done" flag.
      const hasCompilerErrors = failures.filter((f) => !f.stack).length > 0;
      if (hasCompilerErrors) {
        // We can't expect all shots to be done,
        // because of compilation errors.
        resolve({
          status: ProgramRunStatus.CompilationErrors,
          errors: failures,
        });
      } else if (options.shots === resultCount) {
        // All the shots are complete, we're done.
        resolve({
          status: ProgramRunStatus.AllShotsDone,
          shotResults: results.map(
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
        });
      }
    });

    let cancelled = false;
    const worker = loadCompilerWorker(extensionUri!);
    const compilerRunTimeoutMs = 1000 * 60 * 5; // 5 minutes
    const compilerTimeout = setTimeout(() => {
      worker.terminate();
    }, compilerRunTimeoutMs);
    options.cancellationToken?.onCancellationRequested(() => {
      cancelled = true;
      worker.terminate();
    });

    // Invoke the actual compiler worker.
    worker
      .run(program, options.entry || "", options.shots || 1, evtTarget)
      .catch((e) => {
        log.debug("Error during program run:", e);

        const shotResults = evtTarget.getResults().map(
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
        );

        if (e instanceof WebAssembly.RuntimeError) {
          resolve({
            status: ProgramRunStatus.FatalError,
            shotResults,
          });
        } else if (e && e.toString() === "terminated") {
          const doneStatus = cancelled
            ? ProgramRunStatus.Cancellation
            : ProgramRunStatus.Timeout;
          resolve({
            status: doneStatus,
            shotResults,
          });
        } else if (e instanceof Error) {
          // Compiler errors can come through here.
          // But the error object here doesn't contain enough
          // information to be useful, so we use the one that comes
          // through the event target, and let that
          // callback resolve the promise.
        } else {
          // Unknown fatal error
          resolve({
            status: ProgramRunStatus.UnknownError,
            shotResults,
          });
        }
      })
      .finally(() => {
        clearTimeout(compilerTimeout);
        worker.terminate();
      });

    // We can still receive events after  `worker.run` is done,
    // so `worker.run`s continuation is not relevant.
    // The promise will be resolved in the event listener,
    // when all the shots are complete.
  });
}

/**
 * Handles the terminal open event by loading and running the Q# program.
 * Streams console output to the terminal and displays completion status.
 */
async function runOnTerminalOpen(
  document: vscode.Uri,
  extensionUri: vscode.Uri,
  entry: string,
  output: vscode.EventEmitter<string>,
  cancellationToken: vscode.CancellationToken,
): Promise<void> {
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
    cancellationToken,
  });
  if (result.status !== ProgramRunStatus.AllShotsDone) {
    output.fire(`\r\nProgram ended with status: ${result.status}.\r\n`);
  }
  output.fire("\r\nPress any key to close this terminal.\r\n");
}
