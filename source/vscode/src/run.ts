// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { IQSharpError, QdkDiagnostics, log } from "qsharp-lang";
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
  const evtTarget = createDebugConsoleEventTarget((msg) => {
    options.onConsoleOut?.(msg);
  }, true /* captureEvents */);

  // Stream real-time histogram updates during execution.
  evtTarget.addEventListener("uiResultsRefresh", () => {
    const results = evtTarget.getResults();
    const resultCount = evtTarget.resultCount();
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
    options.onResultsUpdate?.(
      {
        buckets: Array.from(buckets.entries()) as [string, number][],
        shotCount: resultCount,
      },
      failures,
    );
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

  try {
    const results = await worker.run(
      program,
      options.entry || "",
      options.shots || 1,
      evtTarget,
    );

    return {
      status: ProgramRunStatus.AllShotsDone,
      shotResults: results.map(
        (r) =>
          (r.success
            ? { success: true, result: r.value }
            : {
                success: false,
                errors: (r.value as { errors: IQSharpError[] }).errors,
              }) as ShotResult,
      ),
    };
  } catch (e) {
    log.debug("Error during program run:", e);

    if (e instanceof QdkDiagnostics) {
      return {
        status: ProgramRunStatus.CompilationErrors,
        errors: e.diagnostics,
      };
    } else if (e instanceof WebAssembly.RuntimeError) {
      return {
        status: ProgramRunStatus.FatalError,
        shotResults: [],
      };
    } else if (e && e.toString() === "terminated") {
      return {
        status: cancelled
          ? ProgramRunStatus.Cancellation
          : ProgramRunStatus.Timeout,
        shotResults: [],
      };
    } else {
      return {
        status: ProgramRunStatus.UnknownError,
        shotResults: [],
      };
    }
  } finally {
    clearTimeout(compilerTimeout);
    worker.terminate();
  }
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
