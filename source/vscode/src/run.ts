// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { IQSharpError } from "qsharp-lang";
import vscode from "vscode";
import { loadCompilerWorker } from "./common";
import { createDebugConsoleEventTarget } from "./debugger/output";
import { FullProgramConfig } from "./programConfig";

export async function runProgram(
  extensionUri: vscode.Uri,
  program: FullProgramConfig,
  entry: string, // can be ""
  shots: number,
  out: (message: string) => void,
  resultUpdate: (histogram: HistogramData, failures: IQSharpError[]) => void,
): Promise<{ failures?: IQSharpError[] }> {
  let histogram: HistogramData | undefined;
  const evtTarget = createDebugConsoleEventTarget((msg) => {
    out(msg);
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
    resultUpdate(histogram!, failures);
    if (shots === resultCount || failures.length > 0) {
      resolvePromise();
    }
  });

  const compilerRunTimeoutMs = 1000 * 60 * 5; // 5 minutes
  const compilerTimeout = setTimeout(() => {
    worker.terminate();
  }, compilerRunTimeoutMs);
  const worker = loadCompilerWorker(extensionUri!);

  try {
    await worker.run(program, entry, shots, evtTarget);
    // We can still receive events after the above call is done
    await allShotsDone;
  } catch {
    // Compiler errors can come through here. But the error object here doesn't contain enough
    // information to be useful. So wait for the one that comes through the event target.
    await allShotsDone;

    const failures = evtTarget
      .getResults()
      .filter((result) => !result.success)
      .flatMap(
        (result) => (result.result as { errors: IQSharpError[] }).errors,
      );

    return {
      failures,
    };
  }
  clearTimeout(compilerTimeout);
  worker.terminate();
  return {};
}

/**
 * Histogram data for displaying results
 */
export type HistogramData = {
  buckets: [string, number][];
  shotCount?: number;
};
