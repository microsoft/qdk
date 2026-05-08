// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { escapeHtml } from "markdown-it/lib/common/utils.mjs";
import {
  type CircuitData,
  ICompilerWorker,
  IOperationInfo,
  IQSharpError,
  QdkDiagnostics,
  log,
} from "qsharp-lang";
import { Uri, window, workspace } from "vscode";
import * as vscode from "vscode";
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
import { basename, loadCompilerWorker } from "./common";

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

  // Stash the latest circuit alongside a sensible default file name so the
  // "Save as Circuit (.qsc)…" button on the webview can post a message
  // back here without having to round-trip the entire payload again.
  if (params.circuit) {
    generatedCircuitCache.set(panelId, {
      circuit: params.circuit,
      suggestedFileName: title || projectName || panelId || "Circuit",
      simulated: params.simulated || false,
    });
  } else if (params.errorHtml || params.calculating) {
    // The cached circuit no longer reflects what's on screen — drop it so
    // the button stays hidden until the next successful generation.
    generatedCircuitCache.delete(panelId);
  }

  const props = {
    title,
    targetProfile: target,
    simulated: params?.simulated || false,
    calculating: params?.calculating || false,
    circuit: params?.circuit,
    errorHtml: params?.errorHtml,
    // The webview swaps this flag for an actual `onSaveAsCircuit` callback
    // before handing props to the React tree (see webview.tsx). We send a
    // boolean rather than a function because postMessage can't ferry
    // closures across the worker boundary.
    canSaveAsCircuit:
      !!params.circuit && !params.calculating && !params.errorHtml,
  };

  const message = {
    props,
  };
  sendMessageToPanel({ panelType: "circuit", id: panelId }, reveal, message);
}

/**
 * Per-panel cache of the most recent successfully generated circuit. Keyed
 * by panelId (the same value the webview echoes back in its messages) so
 * `handleSaveGeneratedCircuit` can locate the right payload without the
 * webview having to round-trip the full circuit on every click.
 */
const generatedCircuitCache = new Map<
  string,
  {
    circuit: CircuitData;
    suggestedFileName: string;
    simulated: boolean;
  }
>();

/**
 * Sanitize a string into a filesystem-safe basename. Strips path separators,
 * Windows-reserved characters, and trims to a reasonable length so the
 * default for the save dialog is always a valid file name.
 */
function sanitizeFileName(name: string): string {
  const cleaned = name
    .replace(/[\\/:*?"<>|]/g, "_")
    .replace(/\s+/g, "_")
    .replace(/^\.+|\.+$/g, "")
    .slice(0, 80);
  return cleaned.length > 0 ? cleaned : "Circuit";
}

/**
 * Choose the directory the save dialog should default to. Prefers the
 * folder of the currently active text editor (so the .qsc lands beside
 * the .qs that produced it), then the first workspace folder, then no
 * default at all.
 */
function defaultSaveDirectory(): Uri | undefined {
  const active = window.activeTextEditor?.document.uri;
  if (active && active.scheme === "file") {
    return Uri.joinPath(active, "..");
  }
  const folder = workspace.workspaceFolders?.[0]?.uri;
  if (folder) return folder;
  return undefined;
}

/**
 * Handler for the "Save as Circuit (.qsc)…" button on the QDK Circuit
 * panel. Writes the cached circuit JSON to a user-chosen .qsc file and
 * opens it with the Circuit Editor, which in turn brings up the live Q#
 * preview alongside (per the existing setting).
 *
 * Best-effort: any failure surfaces as a notification rather than
 * propagating, so a missing cache or denied write doesn't take down the
 * webview.
 */
export async function handleSaveGeneratedCircuit(panelId: string) {
  const cached = generatedCircuitCache.get(panelId);
  if (!cached) {
    void window.showWarningMessage(
      "No circuit available to save yet. Generate the circuit first, then try again.",
    );
    return;
  }

  const baseName = sanitizeFileName(cached.suggestedFileName);
  const baseDir = defaultSaveDirectory();
  const defaultUri = baseDir
    ? Uri.joinPath(baseDir, `${baseName}.qsc`)
    : Uri.file(`${baseName}.qsc`);

  const target = await window.showSaveDialog({
    defaultUri,
    filters: { "Quantum Circuit": ["qsc"] },
    saveLabel: "Save Circuit",
    title: "Save generated circuit as .qsc",
  });
  if (!target) return;

  // The Circuit Editor and the panel's `Circuit` component both accept the
  // CircuitGroup shape `{ version, circuits: [...] }` directly, which is
  // exactly what wasm's getCircuit returns. Strip transient `metadata` (per
  // the schema doc comment, those fields are not meant to be persisted in
  // a .qsc file — they reference the originating .qs by absolute path and
  // can also gain new required-on-read fields between releases). Pretty-
  // print so users can inspect / hand-edit the JSON if they ever crack it
  // open in a text editor.
  const persistable = stripTransientMetadata(cached.circuit);
  const json = JSON.stringify(persistable, null, 2);
  try {
    await workspace.fs.writeFile(target, new TextEncoder().encode(json));
  } catch (err: any) {
    log.error("Failed to write generated circuit to .qsc", err);
    void window.showErrorMessage(
      `Could not save circuit: ${err?.message ?? err}`,
    );
    return;
  }

  try {
    // openWith honours the customEditor registration for *.qsc, which
    // routes through CircuitEditorProvider and (per the user's setting)
    // opens the live Q# preview to the side automatically.
    await vscode.commands.executeCommand(
      "vscode.openWith",
      target,
      "qsharp-webview.circuit",
    );
  } catch (err: any) {
    // Fall back to a plain document open if the custom editor refused for
    // some reason — the file is on disk either way.
    log.warn("openWith(qsharp-webview.circuit) failed, opening as text", err);
    await vscode.commands.executeCommand("vscode.open", target);
  }

  // Trace-derived snapshots are necessarily approximate — surface that to
  // the user once, where they can act on it (the divergence banner inside
  // the live preview will reinforce the same point in code form).
  if (cached.simulated) {
    void window.showInformationMessage(
      "Saved a trace snapshot. The live Q# preview will mark any non-uniform loops or opaque conditionals as approximate.",
    );
  }
}

/**
 * Return a structural copy of `circuit` with all `metadata` fields removed.
 *
 * The Rust `Metadata` struct's doc comment is explicit: "the schema of
 * Metadata may change and its contents are never meant to be persisted in a
 * .qsc file." It also uses `skip_serializing_if = "Vec::is_empty"` on some
 * Vec fields without matching `#[serde(default)]`, which historically
 * produced asymmetric round-trips ("missing field `controlResultIds`")
 * when the trace happened to leave those fields empty.
 *
 * Stripping at the host boundary is the most robust fix: it isolates
 * future Metadata-shape changes from on-disk .qsc files, and means a
 * snapshot saved today will continue to load on a future QDK build that
 * adds new required Metadata fields.
 */
function stripTransientMetadata<T>(circuit: T): T {
  const seen = new WeakSet<object>();
  const visit = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(visit);
    if (value && typeof value === "object") {
      if (seen.has(value as object)) return value;
      seen.add(value as object);
      const out: Record<string, unknown> = {};
      for (const [key, child] of Object.entries(
        value as Record<string, unknown>,
      )) {
        if (key === "metadata") continue; // <-- the strip
        out[key] = visit(child);
      }
      return out;
    }
    return value;
  };
  return visit(circuit) as T;
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
