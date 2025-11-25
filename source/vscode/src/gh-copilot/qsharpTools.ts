// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { TargetProfile, VSDiagnostic } from "qsharp-lang";
import vscode from "vscode";
import { CircuitOrError, showCircuitCommand } from "../circuit";
import { loadCompilerWorker, toVsCodeDiagnostic } from "../common";
import { createDebugConsoleEventTarget } from "../debugger/output";
import { resourceEstimateTool } from "../estimate";
import { FullProgramConfig, getProgramForDocument } from "../programConfig";
import {
  determineDocumentType,
  EventType,
  QsharpDocumentType,
  sendTelemetryEvent,
  UserTaskInvocationType,
} from "../telemetry";
import { getRandomGuid } from "../utils";
import { sendMessageToPanel } from "../webviewPanel.js";
import { CopilotToolError, HistogramData } from "./types";

/**
 * In general, tool calls that deal with Q# should include this project
 * info in their output. Since Copilot just passes in a file path, and isn't
 * familiar with how we expand the project or how we determine target profile,
 * this output will give Copilot context to understand what just happened.
 */
export type ProjectInfo = {
  qsharpProject: {
    name: string;
    targetProfile: string;
  };
};

type RunProgramResult = ProjectInfo &
  (
    | {
        output: string;
        result: string | vscode.Diagnostic;
      }
    | {
        histogram: HistogramData;
        sampleFailures: vscode.Diagnostic[];
        message: string;
      }
  );

export class QSharpTools {
  constructor(
    private extensionUri: vscode.Uri,
    private prerelease: boolean,
  ) {}

  /**
   * Implements the `qdk-run-program` tool call.
   */
  async runProgram(input: {
    filePath: string;
    shots?: number;
  }): Promise<RunProgramResult> {
    const shots = input.shots ?? 1;

    const program = await this.getProgram(input.filePath);
    const programConfig = program.config;

    const output: string[] = [];
    let finalHistogram: HistogramData | undefined;
    let sampleFailures: vscode.Diagnostic[] = [];
    const panelId = programConfig.projectName;

    const start = performance.now();
    const associationId = getRandomGuid();
    if (shots > 1) {
      sendTelemetryEvent(
        EventType.TriggerHistogram,
        {
          associationId,
          documentType: program.telemetryDocumentType,
          invocationType: UserTaskInvocationType.ChatToolCall,
        },
        {},
      );
      sendTelemetryEvent(EventType.HistogramStart, { associationId }, {});
    }

    await this.runQsharp(
      programConfig,
      shots,
      (msg) => {
        output.push(msg);
      },
      (histogram, failures) => {
        finalHistogram = histogram;
        const uniqueFailures = new Set<string>();
        sampleFailures = [];
        for (const failure of failures) {
          const failureKey = `${failure.message}-${failure.range?.start.line}-${failure.range?.start.character}`;
          if (!uniqueFailures.has(failureKey)) {
            uniqueFailures.add(failureKey);
            sampleFailures.push(failure);
          }
          if (sampleFailures.length === 3) {
            break;
          }
        }
        if (
          shots > 1 &&
          histogram.buckets.filter((b) => b[0] !== "ERROR").length > 0
        ) {
          // Display the histogram panel only if we're running multiple shots,
          // and we have at least one successful result.
          sendMessageToPanel(
            { panelType: "histogram", id: panelId },
            true, // reveal the panel
            histogram,
          );
        }
      },
    );

    if (shots > 1) {
      sendTelemetryEvent(
        EventType.HistogramEnd,
        { associationId },
        { timeToCompleteMs: performance.now() - start },
      );
    }

    if (shots === 1) {
      // Return the output and results directly
      return {
        ...program.additionalContextForModel,
        output: output.join("\n"),
        result:
          sampleFailures.length > 0
            ? sampleFailures[0]
            : (finalHistogram?.buckets[0][0] as string),
      };
    } else {
      // No output, return the histogram
      return {
        ...program.additionalContextForModel,
        sampleFailures,
        histogram: finalHistogram!,
        message: `Results are displayed in the Histogram panel.`,
      };
    }
  }

  /**
   * Implements the `qdk-generate-circuit` tool call.
   */
  async generateCircuit(input: { filePath: string }): Promise<
    ProjectInfo &
      CircuitOrError & {
        message?: string;
      }
  > {
    const program = await this.getProgram(input.filePath);
    const programConfig = program.config;

    const circuitOrError = await showCircuitCommand(
      this.extensionUri,
      this.prerelease,
      undefined,
      UserTaskInvocationType.ChatToolCall,
      program.telemetryDocumentType,
      programConfig,
    );

    const result = {
      ...program.additionalContextForModel,
      ...circuitOrError,
    };

    if (circuitOrError.result === "success") {
      return {
        ...result,
        message: "Circuit is displayed in the Circuit panel.",
      };
    } else {
      return {
        ...result,
      };
    }
  }

  /**
   * Implements the `qdk-run-resource-estimator` tool call.
   */
  async runResourceEstimator(input: {
    filePath: string;
    qubitTypes?: string[];
    errorBudget?: number;
  }): Promise<
    ProjectInfo & {
      estimates?: object[];
      message: string;
    }
  > {
    const program = await this.getProgram(input.filePath);
    const programConfig = program.config;

    try {
      const qubitTypes = input.qubitTypes ?? ["qubit_gate_ns_e3"];
      const errorBudget = input.errorBudget ?? 0.001;

      const estimates = await resourceEstimateTool(
        this.extensionUri,
        programConfig,
        program.telemetryDocumentType,
        qubitTypes,
        errorBudget,
      );

      return {
        ...program.additionalContextForModel,
        estimates,
        message: "Results are displayed in the resource estimator panel.",
      };
    } catch (e) {
      throw new CopilotToolError(
        "Failed to run resource estimator: " +
          (e instanceof Error ? e.message : String(e)),
      );
    }
  }

  /**
   * Copilot tool: Returns a Markdown string summarizing all Q# standard library items,
   * organized by namespace. Each entry includes its signature and a short description extracted
   * from doc comments when available.
   */
  async qsharpGetLibraryDescriptions(): Promise<string> {
    const compilerRunTimeoutMs = 1000 * 5; // 5 seconds
    const compilerTimeout = setTimeout(() => {
      worker.terminate();
    }, compilerRunTimeoutMs);
    const worker = loadCompilerWorker(this.extensionUri!);
    const summaries = await worker.getLibrarySummaries();
    clearTimeout(compilerTimeout);
    worker.terminate();
    return summaries;
  }

  async getProgram(
    filePath: string,
    options: { targetProfileFallback?: TargetProfile } = {},
  ): Promise<{
    config: FullProgramConfig;
    telemetryDocumentType: QsharpDocumentType;
    additionalContextForModel: ProjectInfo;
  }> {
    const docUri = vscode.Uri.file(filePath);

    const doc = await vscode.workspace.openTextDocument(docUri);
    const telemetryDocumentType = determineDocumentType(doc);

    const program = await getProgramForDocument(doc, options);
    if (!program.success) {
      throw new CopilotToolError(
        `Cannot get program for the file ${filePath}\n\n${program.diagnostics ? JSON.stringify(program.diagnostics) : program.errorMsg}`,
      );
    }
    return {
      config: program.programConfig,
      telemetryDocumentType,
      additionalContextForModel: {
        qsharpProject: {
          name: program.programConfig.projectName,
          targetProfile: program.programConfig.profile,
        },
      },
    };
  }

  private async runQsharp(
    program: FullProgramConfig,
    shots: number,
    out: (message: string) => void,
    resultUpdate: (
      histogram: HistogramData,
      failures: vscode.Diagnostic[],
    ) => void,
  ) {
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
        const key = results[i].result;
        const strKey = typeof key !== "string" ? "ERROR" : key;
        const newValue = (buckets.get(strKey) || 0) + 1;
        buckets.set(strKey, newValue);
        if (!results[i].success) {
          failures.push(toVsCodeDiagnostic(results[i].result as VSDiagnostic));
        }
      }
      histogram = {
        buckets: Array.from(buckets.entries()) as [string, number][],
        shotCount: resultCount,
      };
      resultUpdate(histogram!, failures);
      if (shots === resultCount || failures.length > 0) {
        // TODO: ugh
        resolvePromise();
      }
    });

    const compilerRunTimeoutMs = 1000 * 60 * 5; // 5 minutes
    const compilerTimeout = setTimeout(() => {
      worker.terminate();
    }, compilerRunTimeoutMs);
    const worker = loadCompilerWorker(this.extensionUri!);

    try {
      await worker.run(program, "", shots, evtTarget);
      // We can still receive events after the above call is done
      await allShotsDone;
    } catch {
      // Compiler errors can come through here. But the error object here doesn't contain enough
      // information to be useful. So wait for the one that comes through the event target.
      await allShotsDone;

      const failures = evtTarget
        .getResults()
        .filter((result) => !result.success)
        .map((result) => toVsCodeDiagnostic(result.result as VSDiagnostic));

      throw new CopilotToolError(
        `Program failed with compilation errors. ${JSON.stringify(failures)}`,
      );
    }
    clearTimeout(compilerTimeout);
    worker.terminate();
  }
}
