// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { escapeHtml } from "markdown-it/lib/common/utils.mjs";
import {
  type CircuitData,
  ICompilerWorker,
  IOperationInfo,
  IQSharpError,
  IRange,
  QdkDiagnostics,
  getCompilerWorker,
  log,
} from "qsharp-lang";
import { Uri } from "vscode";
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

  if (!programConfig) {
    const program = await getActiveProgram({ showModalError: true });
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
  const result = await generateCircuit(extensionUri, {
    program: programConfig,
    operation,
  });

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
export async function generateCircuit(
  extensionUri: Uri,
  params: CircuitParams,
): Promise<CircuitOrError> {
  // Before we start, reveal the panel with the "calculating" spinner
  updateCircuitPanel(
    params.program.profile,
    params.program.projectName,
    true, // reveal
    { operation: params.operation, calculating: true },
  );

  // First, try without simulating
  let result = await getCircuitOrErrorWithTimeout(
    extensionUri,
    params,
    false, // simulate
  );

  if (result.result === "error" && result.hasResultComparisonError) {
    // Retry with the simulator if circuit generation failed because
    // there was a result comparison (i.e. if this is a dynamic circuit)

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
    result = await getCircuitOrErrorWithTimeout(
      extensionUri,
      params,
      true, // simulate
    );
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
  simulate: boolean,
  timeoutMs: number = compilerRunTimeoutMs,
): Promise<CircuitOrError> {
  let timeout = false;

  const compilerWorkerScriptPath = Uri.joinPath(
    extensionUri,
    "./out/compilerWorker.js",
  ).toString();

  const worker = getCompilerWorker(compilerWorkerScriptPath);
  const compilerTimeout = setTimeout(() => {
    timeout = true;
    log.info("terminating circuit worker due to timeout");
    worker.terminate();
  }, timeoutMs);

  const result = await getCircuitOrError(worker, params, simulate);
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
  simulate: boolean,
): Promise<CircuitOrError> {
  try {
    const circuit = await worker.getCircuit(
      params.program,
      simulate,
      params.operation,
    );
    return { result: "success", simulated: simulate, circuit: circuit };
  } catch (e: any) {
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
      simulated: simulate,
      errors,
      hasResultComparisonError: resultCompError,
      timeout: false,
    };
  }
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

/**
 * Formats an array of compiler/runtime errors into HTML to be presented to the user.
 *
 * @param errors The list of errors to format.
 * @returns The HTML formatted errors, to be set as the inner contents of a container element.
 */
function errorsToHtml(errors: IQSharpError[]) {
  let errorHtml = "";
  for (const error of errors) {
    const { document, diagnostic: diag, stack: rawStack } = error;

    const location = documentHtml(document, diag.range);
    const message = escapeHtml(`(${diag.code}) ${diag.message}`).replace(
      /\n/g,
      "<br/><br/>",
    );

    errorHtml += `<p>${location}: ${message}<br/></p>`;

    if (rawStack) {
      const stack = rawStack
        .split("\n")
        .map((l) => {
          // Link-ify the document names in the stack trace
          const match = l.match(/^(\s*)at (.*) in (.*)/);
          if (match) {
            const [, leadingWs, callable, doc] = match;
            return `${leadingWs}at ${escapeHtml(callable)} in ${documentHtml(doc)}`;
          } else {
            return l;
          }
        })

        .join("\n");
      errorHtml += `<br/><pre>${stack}</pre>`;
    }
  }
  return errorHtml;
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
    ? `${params.operation.operation} with ${params.operation.totalNumQubits} input qubits`
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

/**
 * If the input is a URI, turns it into a document open link.
 * Otherwise returns the HTML-escaped input
 */
function documentHtml(maybeUri: string, range?: IRange) {
  let location;
  try {
    // If the error location is a document URI, create a link to that document.
    // We use the `vscode.open` command (https://code.visualstudio.com/api/references/commands#commands)
    // to open the document in the editor.
    // The line and column information is displayed, but are not part of the link.
    //
    // At the time of writing this is the only way we know to create a direct
    // link to a Q# document from a Web View.
    //
    // If we wanted to handle line/column information from the link, an alternate
    // implementation might be having our own command that navigates to the correct
    // location. Then this would be a link to that command instead. Yet another
    // alternative is to have the webview pass a message back to the extension.
    const uri = Uri.parse(maybeUri, true);
    const openCommandUri = Uri.parse(
      `command:vscode.open?${encodeURIComponent(JSON.stringify([uri]))}`,
      true,
    );
    const fsPath = escapeHtml(uri.fsPath);
    const lineColumn = range
      ? escapeHtml(`:${range.start.line + 1}:${range.start.character + 1}`)
      : "";
    location = `<a href="${openCommandUri}">${fsPath}</a>${lineColumn}`;
  } catch {
    // Likely could not parse document URI - it must be a project level error
    // or an error from stdlib, use the document name directly
    location = escapeHtml(maybeUri);
  }

  return location;
}
