// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";
import { window } from "vscode";
import { loadCompilerWorker } from "./common";
import { clearCommandDiagnostics } from "./diagnostics";
import { FullProgramConfig, getActiveProgram } from "./programConfig";
import {
  UserTaskInvocationType,
  EventType,
  getActiveDocumentType,
  QsharpDocumentType,
  sendTelemetryEvent,
} from "./telemetry";
import { getRandomGuid } from "./utils";
import { getOrCreatePanel, sendMessageToPanel } from "./webviewPanel";

const compilerRunTimeoutMs = 1000 * 60 * 5; // 5 minutes

export async function resourceEstimateCommand(
  extensionUri: vscode.Uri,
  resource?: vscode.Uri,
  expr?: string,
) {
  clearCommandDiagnostics();
  const associationId = getRandomGuid();
  sendTelemetryEvent(
    EventType.TriggerResourceEstimation,
    {
      associationId,
      documentType: getActiveDocumentType(),
      invocationType: UserTaskInvocationType.Command,
    },
    {},
  );

  const program = await getActiveProgram({ showModalError: true });
  if (!program.success) {
    throw new Error(program.errorMsg);
  }

  const qubitType = await window.showQuickPick(qubitTypeOptions, {
    canPickMany: true,
    title: "Qubit types",
    placeHolder: "Superconducting/spin qubit with 1e-3 error rate",
    matchOnDetail: true,
  });

  if (!qubitType) {
    return;
  }

  // Prompt for error budget (default to 0.001)
  const validateErrorBudget = (input: string) => {
    const result = parseFloat(input);
    if (isNaN(result) || result <= 0.0 || result >= 1.0) {
      return "Error budgets must be between 0 and 1";
    }
  };

  const errorBudget = await window.showInputBox({
    value: "0.001",
    prompt: "Error budget",
    validateInput: validateErrorBudget,
  });

  // abort if the user hits <Esc> during shots entry
  if (errorBudget === undefined) {
    return;
  }

  const runName = await window.showInputBox({
    title: "Friendly name for run",
    value: `${program.programConfig.projectName}`,
  });
  if (!runName) {
    return;
  }

  await executeResourceEstimation(
    extensionUri,
    runName,
    associationId,
    program.programConfig,
    expr,
    qubitType,
    parseFloat(errorBudget),
  );
}

const qubitTypeOptions = [
  {
    label: "qubit_gate_ns_e3",
    detail: "Superconducting/spin qubit with 1e-3 error rate",
    picked: true,
    params: {
      qubitParams: { name: "qubit_gate_ns_e3" },
      qecScheme: { name: "surface_code" },
    },
  },
  {
    label: "qubit_gate_ns_e4",
    detail: "Superconducting/spin qubit with 1e-4 error rate",
    params: {
      qubitParams: { name: "qubit_gate_ns_e4" },
      qecScheme: { name: "surface_code" },
    },
  },
  {
    label: "qubit_gate_us_e3",
    detail: "Trapped ion qubit with 1e-3 error rate",
    params: {
      qubitParams: { name: "qubit_gate_us_e3" },
      qecScheme: { name: "surface_code" },
    },
  },
  {
    label: "qubit_gate_us_e4",
    detail: "Trapped ion qubit with 1e-4 error rate",
    params: {
      qubitParams: { name: "qubit_gate_us_e4" },
      qecScheme: { name: "surface_code" },
    },
  },
  {
    label: "qubit_maj_ns_e4 + surface_code",
    detail: "Majorana qubit with 1e-4 error rate (surface code QEC)",
    params: {
      qubitParams: { name: "qubit_maj_ns_e4" },
      qecScheme: { name: "surface_code" },
    },
  },
  {
    label: "qubit_maj_ns_e6 + surface_code",
    detail: "Majorana qubit with 1e-6 error rate (surface code QEC)",
    params: {
      qubitParams: { name: "qubit_maj_ns_e6" },
      qecScheme: { name: "surface_code" },
    },
  },
  {
    label: "qubit_maj_ns_e4 + floquet_code",
    detail: "Majorana qubit with 1e-4 error rate (floquet code QEC)",
    params: {
      qubitParams: { name: "qubit_maj_ns_e4" },
      qecScheme: { name: "floquet_code" },
    },
  },
  {
    label: "qubit_maj_ns_e6 + floquet_code",
    detail: "Majorana qubit with 1e-6 error rate (floquet code QEC)",
    params: {
      qubitParams: { name: "qubit_maj_ns_e6" },
      qecScheme: { name: "floquet_code" },
    },
  },
];

export function resourceEstimateTool(
  extensionUri: vscode.Uri,
  programConfig: FullProgramConfig,
  telemetryDocumentType: QsharpDocumentType,
  qubitTypeLabels: string[],
  errorBudget: number,
): Promise<object[] | undefined> {
  const selectedQubitTypes = qubitTypeOptions.filter((option) =>
    qubitTypeLabels.includes(option.label),
  );

  const runName = programConfig.projectName || "ResourceEstimation";
  const associationId = getRandomGuid();
  sendTelemetryEvent(
    EventType.TriggerResourceEstimation,
    {
      associationId,
      documentType: telemetryDocumentType,
      invocationType: UserTaskInvocationType.ChatToolCall,
    },
    {},
  );

  return executeResourceEstimation(
    extensionUri,
    runName,
    associationId,
    programConfig,
    undefined,
    selectedQubitTypes,
    errorBudget,
  );
}

async function executeResourceEstimation(
  extensionUri: vscode.Uri,
  runName: string,
  associationId: string,
  programConfig: FullProgramConfig,
  expr: string | undefined,
  qubitType: {
    label: string;
    detail: string;
    params: {
      qubitParams: {
        name: string;
      };
      qecScheme: {
        name: string;
      };
    };
  }[],
  errorBudget: number,
): Promise<object[] | undefined> {
  const params = qubitType.map((item) => ({
    ...item.params,
    errorBudget,
    estimateType: "frontier",
  }));

  log.info("RE params", params);

  sendMessageToPanel({ panelType: "estimates" }, true, {
    calculating: true,
  });

  const estimatePanel = getOrCreatePanel("estimates");
  // Ensure the name is unique
  if (estimatePanel.state[runName] !== undefined) {
    let idx = 2;
    for (;;) {
      const newName = `${runName}-${idx}`;
      if (estimatePanel.state[newName] === undefined) {
        runName = newName;
        break;
      }
      idx++;
    }
  }
  estimatePanel.state[runName] = true;

  // Start the worker, run the code, and send the results to the webview
  log.debug("Starting resource estimates worker.");
  let timedOut = false;

  const worker = loadCompilerWorker(extensionUri);
  const compilerTimeout = setTimeout(() => {
    log.info("Compiler timeout. Terminating worker.");
    timedOut = true;
    worker.terminate();
  }, compilerRunTimeoutMs);

  try {
    const start = performance.now();
    sendTelemetryEvent(
      EventType.ResourceEstimationStart,
      { associationId },
      {},
    );
    const estimatesStr = await worker.getEstimates(
      programConfig,
      expr ?? "",
      JSON.stringify(params),
    );
    sendTelemetryEvent(
      EventType.ResourceEstimationEnd,
      { associationId },
      { timeToCompleteMs: performance.now() - start },
    );
    log.debug("Estimates result", estimatesStr);

    // Should be an array of one ReData object returned
    const estimates = JSON.parse(estimatesStr);

    for (const item of estimates) {
      // if item doesn't have a status property, it's an error
      if (!("status" in item) || item.status !== "success") {
        log.error("Estimates error code: ", item.code);
        log.error("Estimates error message: ", item.message);
        throw item.message;
      }
    }

    (estimates as Array<any>).forEach(
      (item) => (item.jobParams.sharedRunName = runName),
    );

    clearTimeout(compilerTimeout);

    const message = {
      calculating: false,
      estimates,
    };
    sendMessageToPanel({ panelType: "estimates" }, true, message);

    return estimates;
  } catch (e: any) {
    // Stop the 'calculating' animation
    const message = {
      calculating: false,
      estimates: [],
    };
    sendMessageToPanel({ panelType: "estimates" }, false, message);

    if (timedOut) {
      // Show a VS Code popup that a timeout occurred
      window.showErrorMessage(
        "The resource estimation timed out. Please try again.",
      );
    } else {
      log.error("getEstimates error: ", e.toString());
      throw new Error("Estimating failed with error: " + e.toString(), {
        cause: e,
      });
    }
  } finally {
    if (!timedOut) {
      log.debug("Terminating resource estimates worker.");
      worker.terminate();
    }
  }
}
