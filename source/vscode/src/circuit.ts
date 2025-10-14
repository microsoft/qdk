// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { escapeHtml } from "markdown-it/lib/common/utils.mjs";
import {
  type CircuitData,
  ICompilerWorker,
  IOperationInfo,
  IQSharpError,
  QdkDiagnostics,
  getCompilerWorker,
  log,
} from "qsharp-lang";
import { Uri, workspace } from "vscode";
import { ComponentGrid } from "../../npm/qsharp/dist/data-structures/circuit";
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
import { ICircuitConfig, IPosition } from "../../npm/qsharp/lib/web/qsc_wasm";

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
    result.hasResultComparisonError &&
    config.generationMethod === "classicalEval"
  ) {
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
      params.operation,
      config,
    );
    transformLocations(circuit);
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
  const defaultConfig = {
    maxOperations: 10001,
    loopDetection: false,
    groupScopes: true,
    generationMethod: "static" as const,
    collapseQubitRegisters: false,
  };

  const config = workspace
    .getConfiguration("Q#")
    .get<object>("circuits.config", defaultConfig);

  const configObject = {
    maxOperations:
      "maxOperations" in config && typeof config.maxOperations === "number"
        ? config.maxOperations
        : defaultConfig.maxOperations,
    loopDetection:
      "loopDetection" in config && typeof config.loopDetection === "boolean"
        ? config.loopDetection
        : defaultConfig.loopDetection,
    groupScopes:
      "groupScopes" in config && typeof config.groupScopes === "boolean"
        ? config.groupScopes
        : defaultConfig.groupScopes,
    generationMethod:
      "generationMethod" in config &&
      typeof config.generationMethod === "string" &&
      ["simulate", "classicalEval", "static"].includes(config.generationMethod)
        ? (config.generationMethod as "simulate" | "classicalEval" | "static")
        : defaultConfig.generationMethod,
    collapseQubitRegisters:
      "collapseQubitRegisters" in config &&
      typeof config.collapseQubitRegisters === "boolean"
        ? config.collapseQubitRegisters
        : defaultConfig.collapseQubitRegisters,
  };

  log.debug("Using circuit config: ", configObject);
  return configObject;
}

function transformLocations(circuits: CircuitData) {
  for (const circuit of circuits.circuits) {
    const componentGrid = circuit.componentGrid;
    mapComponentLocationsToHtml(componentGrid);
  }
}

function mapComponentLocationsToHtml(componentGrid: ComponentGrid) {
  log.debug(
    "Mapping component locations to HTML for component grid: ",
    componentGrid,
  );
  for (const column of componentGrid) {
    for (const component of column.components) {
      if (component.children?.length) {
        mapComponentLocationsToHtml(component.children);
      }

      const source = component.source;
      if (source) {
        const html = documentHtml(true, source.file, {
          line: source.line,
          character: source.column,
        });
        if (!component.args) {
          component.args = [];
        }
        component.args.push(html);
      }
    }
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

    const location = documentHtml(false, document, diag.range.start);
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
            return `${leadingWs}at ${escapeHtml(callable)} in ${documentHtml(false, doc)}`;
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
function documentHtml(
  customCommand: boolean,
  maybeUri: string,
  position?: IPosition,
) {
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

    if (customCommand && position) {
      const openCommandUri = `command:qsharp-vscode.gotoLocation?${encodeURIComponent(JSON.stringify([uri, position]))}`;
      location = `<a href="${openCommandUri}">source</a>`;
    } else {
      const args = [uri];
      const command = "vscode.open";
      const openCommandUri = Uri.parse(
        `command:${command}?${encodeURIComponent(JSON.stringify(args))}`,
        true,
      );
      const fsPath = escapeHtml(uri.fsPath);
      const lineColumn = position
        ? escapeHtml(`:${position.line + 1}:${position.character + 1}`)
        : "";
      location = `<a href="${openCommandUri}">${fsPath}</a>${lineColumn}`;
    }
  } catch {
    // Likely could not parse document URI - it must be a project level error
    // or an error from stdlib, use the document name directly
    location = escapeHtml(maybeUri);
  }

  return location;
}
