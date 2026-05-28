// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  type CircuitData,
  ICompilerWorker,
  IOperationInfo,
  IQSharpError,
  QdkDiagnostics,
  log,
} from "qsharp-lang";
import { Uri, workspace } from "vscode";
import { getTargetFriendlyName } from "./config";
import { clearCommandDiagnostics } from "./diagnostics";
import { FullProgramConfig, getActiveProgram } from "./programConfig";
import {
  EventType,
  QsharpDocumentType,
  UserFlowStatus,
  UserTaskInvocationType,
  getActiveDocumentType,
  sendTelemetryEvent,
} from "./telemetry";
import { getRandomGuid } from "./utils";
import { sendMessageToPanel } from "./webviewPanel";
import { ICircuitConfig } from "../../npm/qsharp/lib/web/qsc_wasm";
import { errorsToHtml, loadCompilerWorker } from "./common";

const compilerRunTimeoutMs = 1000 * 60 * 5; // 5 minutes

/**
 * Input parameters for generating a circuit.
 */
type CircuitParams = {
  program: FullProgramConfig;
  operation?: IOperationInfo;
};

/**
 * Result of a circuit generation attempt.
 */
export type CircuitOrError = {
  simulated: boolean;
} & (
  | {
      result: "success";
      circuit: CircuitData;
    }
  | {
      result: "error";
      errors: IQSharpError[];
      hasResultComparisonError: boolean;
      timeout: boolean;
    }
);

export async function showCircuitCommand(
  extensionUri: Uri,
  operation: IOperationInfo | undefined,
  telemetryInvocationType: UserTaskInvocationType,
  telemetryDocumentType?: QsharpDocumentType,
  programConfig?: FullProgramConfig,
): Promise<CircuitOrError> {
  clearCommandDiagnostics();

  const associationId = getRandomGuid();
  sendTelemetryEvent(
    EventType.TriggerCircuit,
    {
      documentType: telemetryDocumentType || getActiveDocumentType(),
      associationId,
      invocationType: telemetryInvocationType,
    },
    {},
  );

  const circuitConfig = getConfig();
  if (!programConfig) {
    const targetProfileFallback =
      circuitConfig.generationMethod === "static" ? "adaptive_rif" : undefined;
    const program = await getActiveProgram({
      showModalError: true,
      targetProfileFallback,
    });
    if (!program.success) {
      throw new Error(program.errorMsg);
    }
    programConfig = program.programConfig;
  }

  sendTelemetryEvent(
    EventType.CircuitStart,
    {
      associationId,
      targetProfile: programConfig.profile,
      isOperation: (!!operation).toString(),
    },
    {},
  );

  // Generate the circuit and update the panel.
  // generateCircuits() takes care of handling timeouts and
  // falling back to the simulator for dynamic circuits.
  const result = await generateCircuit(
    extensionUri,
    {
      program: programConfig,
      operation,
    },
    circuitConfig,
  );

  if (result.result === "success") {
    sendTelemetryEvent(EventType.CircuitEnd, {
      simulated: result.simulated.toString(),
      associationId,
      flowStatus: UserFlowStatus.Succeeded,
    });
  } else {
    if (result.timeout) {
      sendTelemetryEvent(EventType.CircuitEnd, {
        simulated: result.simulated.toString(),
        associationId,
        reason: "timeout",
        flowStatus: UserFlowStatus.Aborted,
      });
    } else {
      const reason =
        result.errors.length > 0 ? result.errors[0].diagnostic.code : "unknown";

      sendTelemetryEvent(EventType.CircuitEnd, {
        simulated: result.simulated.toString(),
        associationId,
        reason,
        flowStatus: UserFlowStatus.Failed,
      });
    }
  }

  return result;
}

/**
 * Generate the circuit and update the panel with the results.
 * We first attempt to generate a circuit without running the simulator,
 * which should be fast.
 *
 * If that fails, specifically due to a result comparison error,
 * that means this is a dynamic circuit. We fall back to using the
 * simulator in this case ("trace" mode), which is slower.
 */
async function generateCircuit(
  extensionUri: Uri,
  params: CircuitParams,
  config: ICircuitConfig,
): Promise<CircuitOrError> {
  // Before we start, reveal the panel with the "calculating" spinner
  updateCircuitPanel(
    params.program.profile,
    params.program.projectName,
    true, // reveal
    { operation: params.operation, calculating: true },
  );

  // First, try with given config (static by default)
  let result = await getCircuitOrErrorWithTimeout(extensionUri, params, config);

  if (
    result.result === "error" &&
    hasAdaptiveComplianceOrUnsupportedRirError(result.errors) &&
    config.generationMethod === "static"
  ) {
    // Retry with "classicalEval" method if the "static" method failed due to issues like QIR Adaptive compliance
    // Note: this will fall back to TargetProfile="Unrestricted" even if the program explicitly specifies "Adaptive" in its configuration.
    // This may not be desirable behavior. However, retrieving the configuration again here via `getActiveProgram` would require parsing
    // the whole program again so we take this shortcut for now.
    log.debug(
      "Retrying circuit generation with classicalEval due to errors: ",
      result.errors,
    );

    params.program.profile = "unrestricted";
    config.generationMethod = "classicalEval";

    // Force the panel open
    updateCircuitPanel(
      params.program.profile,
      params.program.projectName,
      false, // reveal
      {
        operation: params.operation,
        calculating: true,
        simulated: false,
      },
    );

    // try again with the new settings
    result = await getCircuitOrErrorWithTimeout(extensionUri, params, config);
  }

  if (
    result.result === "error" &&
    result.hasResultComparisonError &&
    config.generationMethod === "classicalEval"
  ) {
    // Retry with the simulator if circuit generation failed because
    // there was a result comparison (i.e. if this is a dynamic circuit)
    log.debug(
      "Retrying circuit generation with simulation due to result comparison error: ",
      result.errors,
    );

    updateCircuitPanel(
      params.program.profile,
      params.program.projectName,
      false, // reveal
      {
        operation: params.operation,
        calculating: true,
        simulated: true,
      },
    );

    // try again with the simulator
    config.generationMethod = "simulate";

    result = await getCircuitOrErrorWithTimeout(extensionUri, params, config);
  }

  // Update the panel with the results

  if (result.result === "success") {
    updateCircuitPanel(
      params.program.profile,
      params.program.projectName,
      false, // reveal
      {
        circuit: result.circuit,
        operation: params.operation,
        simulated: result.simulated,
      },
    );
  } else {
    log.error("Circuit error. ", result);
    let errorHtml = "There was an error generating the circuit.";
    if (result.errors.length > 0) {
      errorHtml = errorsToHtml(result.errors);
    } else if (result.timeout) {
      errorHtml = `The circuit generation exceeded the timeout of ${compilerRunTimeoutMs}ms.`;
    }

    updateCircuitPanel(
      params.program.profile,
      params.program.projectName,
      false, // reveal
      {
        errorHtml,
        operation: params.operation,
        simulated: result.simulated,
      },
    );
  }

  return result;
}

/**
 * Wrapper around getCircuit() that enforces a timeout.
 * Won't throw for known errors.
 */
export async function getCircuitOrErrorWithTimeout(
  extensionUri: Uri,
  params: CircuitParams,
  config: ICircuitConfig,
  timeoutMs: number = compilerRunTimeoutMs,
): Promise<CircuitOrError> {
  let timeout = false;

  const worker = loadCompilerWorker(extensionUri);
  const compilerTimeout = setTimeout(() => {
    timeout = true;
    log.info("terminating circuit worker due to timeout");
    worker.terminate();
  }, timeoutMs);

  const result = await getCircuitOrError(worker, params, config);
  clearTimeout(compilerTimeout);

  if (result.result === "error") {
    return {
      ...result,
      timeout,
    };
  } else {
    return result;
  }
}

/**
 * Wrapper around compiler getCircuit() that handles exceptions
 * and converts to strongly typed error object.
 * Won't throw for known errors.
 */
async function getCircuitOrError(
  worker: ICompilerWorker,
  params: CircuitParams,
  config: ICircuitConfig,
): Promise<CircuitOrError> {
  try {
    const circuit = await worker.getCircuit(
      params.program,
      config,
      params.operation,
    );
    return {
      result: "success",
      simulated: config.generationMethod === "simulate",
      circuit: circuit,
    };
  } catch (e: any) {
    log.error("Error generating circuit: ", e);
    let errors: IQSharpError[] = [];
    let resultCompError = false;
    if (e instanceof QdkDiagnostics) {
      try {
        errors = e.diagnostics;
        resultCompError = hasResultComparisonError(errors);
      } catch {
        // couldn't parse the error - would indicate a bug.
        // will get reported up the stack as a generic error
      }
    }
    return {
      result: "error",
      simulated: config.generationMethod === "simulate",
      errors,
      hasResultComparisonError: resultCompError,
      timeout: false,
    };
  }
}

export function getConfig() {
  // These defaults should match those in `package.json`
  const defaultConfig = {
    maxOperations: 10001,
    groupByScope: true,
    generationMethod: "static" as const,
    sourceLocations: true,
  };

  const config = workspace
    .getConfiguration("Q#")
    .get<object>("circuits.config", defaultConfig);

  const configObject = {
    maxOperations:
      "maxOperations" in config && typeof config.maxOperations === "number"
        ? config.maxOperations
        : defaultConfig.maxOperations,
    groupByScope:
      "groupByScope" in config && typeof config.groupByScope === "boolean"
        ? config.groupByScope
        : defaultConfig.groupByScope,
    generationMethod:
      "generationMethod" in config &&
      typeof config.generationMethod === "string" &&
      ["simulate", "classicalEval", "static"].includes(config.generationMethod)
        ? (config.generationMethod as "simulate" | "classicalEval" | "static")
        : defaultConfig.generationMethod,
    sourceLocations:
      "sourceLocations" in config && typeof config.sourceLocations === "boolean"
        ? config.sourceLocations
        : defaultConfig.sourceLocations,
  };

  log.debug("Using circuit config: ", configObject);
  return configObject;
}

function hasResultComparisonError(errors: IQSharpError[]) {
  const hasResultComparisonError =
    errors &&
    errors.findIndex(
      (item) =>
        item?.diagnostic?.code === "Qsc.Eval.ResultComparisonUnsupported",
    ) >= 0;
  return hasResultComparisonError;
}

function hasAdaptiveComplianceOrUnsupportedRirError(errors: IQSharpError[]) {
  const hasResultComparisonError =
    errors &&
    errors.findIndex((item) => {
      const code = item?.diagnostic?.code;
      return (
        !!code &&
        (code.startsWith("Qsc.PartialEval.") || // Partial eval error (codegen)
          code.startsWith("Qsc.CapabilitiesCk.") || // RCA error (codegen)
          code === "Qsc.Resolve.NotFound" || // Raised sometimes when @Config(Unrestricted) items are used in Adaptive
          code === "Qsc.Circuit.UnsupportedFeature") // Generated RIR can't be handled by the circuit generator
      );
    }) >= 0;
  return hasResultComparisonError;
}

export function updateCircuitPanel(
  targetProfile: string,
  projectName: string,
  reveal: boolean,
  params: {
    circuit?: CircuitData;
    errorHtml?: string;
    simulated?: boolean;
    operation?: IOperationInfo | undefined;
    calculating?: boolean;
  },
) {
  const panelId = params?.operation?.operation || projectName;
  const title = params?.operation
    ? params.operation.totalNumQubits > 0
      ? `${params.operation.operation} with ${params.operation.totalNumQubits} input qubits`
      : params.operation.operation
    : projectName;

  const target = `Target profile: ${getTargetFriendlyName(targetProfile)} `;

  const props = {
    title,
    targetProfile: target,
    simulated: params?.simulated || false,
    calculating: params?.calculating || false,
    circuit: params?.circuit,
    errorHtml: params?.errorHtml,
  };

  const message = {
    props,
  };
  sendMessageToPanel({ panelType: "circuit", id: panelId }, reveal, message);
}
