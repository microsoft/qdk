// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { escapeHtml } from "markdown-it/lib/common/utils.mjs";
import {
  ICompilerWorker,
  IOperationInfo,
  IQSharpError,
  QdkDiagnostics,
  getCompilerWorker,
  log,
} from "qsharp-lang";
import { Uri, workspace } from "vscode";
import { getTargetFriendlyName } from "./config";
import { clearCommandDiagnostics } from "./diagnostics";
import { FullProgramConfig, getActiveProgram } from "./programConfig";
import { QsharpDocumentType, UserTaskInvocationType } from "./telemetry";
import { sendMessageToPanel } from "./webviewPanel";
import {
  ICircuitConfig,
  IPosition,
  IStacks,
} from "../../npm/qsharp/lib/web/qsc_wasm";
import { basename } from "./common";

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
export type StacksOrError = {
  simulated: boolean;
} & (
  | {
      result: "success";
      stacks: IStacks;
    }
  | {
      result: "error";
      errors: IQSharpError[];
      hasResultComparisonError: boolean;
      timeout: boolean;
    }
);

export async function showFlamegraphCommand(
  extensionUri: Uri,
  prerelease: boolean,
  operation: IOperationInfo | undefined,
  telemetryInvocationType: UserTaskInvocationType,
  telemetryDocumentType?: QsharpDocumentType,
  programConfig?: FullProgramConfig,
): Promise<StacksOrError> {
  clearCommandDiagnostics();

  const circuitConfig = getConfig(prerelease);
  if (!programConfig) {
    const program = await getActiveProgram({ showModalError: true });
    if (!program.success) {
      throw new Error(program.errorMsg);
    }
    programConfig = program.programConfig;
  }

  // Generate the circuit and update the panel.
  // generateCircuits() takes care of handling timeouts and
  // falling back to the simulator for dynamic circuits.
  const result = await generateFlamegraph(
    extensionUri,
    {
      program: programConfig,
      operation,
    },
    circuitConfig,
  );

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
async function generateFlamegraph(
  extensionUri: Uri,
  params: CircuitParams,
  config: ICircuitConfig,
): Promise<StacksOrError> {
  // Before we start, reveal the panel with the "calculating" spinner
  updateFlamegraphPanel(
    params.program.profile,
    params.program.projectName,
    true, // reveal
    { operation: params.operation, calculating: true },
  );

  // First, try with given config (classicalEval by default)
  let result = await getCircuitOrErrorWithTimeout(extensionUri, params, config);

  if (
    result.result === "error" &&
    result.hasResultComparisonError &&
    config.generationMethod === "classicalEval"
  ) {
    // Retry with the simulator if circuit generation failed because
    // there was a result comparison (i.e. if this is a dynamic circuit)

    updateFlamegraphPanel(
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
    updateFlamegraphPanel(
      params.program.profile,
      params.program.projectName,
      false, // reveal
      {
        stacks: result.stacks,
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

    updateFlamegraphPanel(
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
): Promise<StacksOrError> {
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
): Promise<StacksOrError> {
  try {
    const stacks = await worker.getFlamegraph(
      params.program,
      config,
      params.operation,
    );
    return {
      result: "success",
      simulated: config.generationMethod === "simulate",
      stacks: stacks,
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

export function getConfig(prerelease: boolean) {
  const defaultConfig = {
    maxOperations: 10001,
    groupScopes: prerelease ? true : false,
    generationMethod: "classicalEval" as const,
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
    groupScopes:
      "groupScopes" in config && typeof config.groupScopes === "boolean"
        ? config.groupScopes
        : defaultConfig.groupScopes,
    generationMethod:
      "generationMethod" in config &&
      typeof config.generationMethod === "string" &&
      ["simulate", "classicalEval"].includes(config.generationMethod)
        ? (config.generationMethod as "simulate" | "classicalEval")
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
          const match = l.match(/^(\s*)at (.*) in (.*):(\d+):(\d+)/);
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

function updateFlamegraphPanel(
  targetProfile: string,
  projectName: string,
  reveal: boolean,
  params: {
    stacks?: IStacks;
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
    stacks: params?.stacks,
    errorHtml: params?.errorHtml,
  };

  const message = {
    props,
  };
  sendMessageToPanel({ panelType: "flamegraph", id: panelId }, reveal, message);
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
    const fsPath = escapeHtml(basename(uri.path) ?? uri.fsPath);
    const lineColumn = position
      ? escapeHtml(`:${position.line + 1}:${position.character + 1}`)
      : "";

    const locations = [
      {
        source: uri,
        span: {
          start: position,
          end: position,
        },
      },
    ];

    const args = customCommand && position ? [locations] : [uri];
    const openCommand =
      customCommand && position ? "qsharp-vscode.gotoLocations" : "vscode.open";

    const argsStr = encodeURIComponent(JSON.stringify(args));
    const openCommandUri = Uri.parse(`command:${openCommand}?${argsStr}`, true);
    const title = `${fsPath}${lineColumn}`;
    return `<a href="${openCommandUri}">${title}</a>`;
  } catch {
    // Likely could not parse document URI - it must be a project level error
    // or an error from stdlib, use the document name directly
    return escapeHtml(maybeUri);
  }
}
