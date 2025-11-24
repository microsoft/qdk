// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { Dump, IQSharpError, QscEventTarget } from "qsharp-lang";
import { getSourceUri } from "../utils";

function formatComplex(real: number, imag: number) {
  // Format -0 as 0
  // Also using Unicode Minus Sign instead of ASCII Hyphen-Minus
  // and Unicode Mathematical Italic Small I instead of ASCII i.
  const r = `${real <= -0.00005 ? "−" : " "}${Math.abs(real).toFixed(4)}`;
  const i = `${imag <= -0.00005 ? "−" : "+"}${Math.abs(imag).toFixed(4)}𝑖`;
  return `${r}${i}`;
}

/**
 * Compiler event target that turns events into well-formatted console output.
 */
export function createDebugConsoleEventTarget(
  out: (message: string) => void,
  captureEvents: boolean = false,
) {
  const eventTarget = new QscEventTarget(captureEvents);

  eventTarget.addEventListener("Message", (evt) => {
    out(evt.detail + "\n");
  });

  eventTarget.addEventListener("DumpMachine", (evt) => {
    out(formatQuantumState(evt) + "\n");
  });

  eventTarget.addEventListener("Matrix", (evt) => {
    out(formatMatrix(evt) + "\n");
  });

  eventTarget.addEventListener("Result", (evt) => {
    if (evt.detail.success) {
      out(`${evt.detail.value}`);
    } else {
      out(formatErrors(evt.detail.value.errors));
    }
  });

  return eventTarget;
}

function formatProbabilityPercent(real: number, imag: number) {
  const probabilityPercent = (real * real + imag * imag) * 100;
  return `${probabilityPercent.toFixed(4)}%`;
}

function formatPhase(real: number, imag: number) {
  const phase = Math.atan2(imag, real);
  return phase.toFixed(4);
}

function formatQuantumState(
  evt: Event & {
    type: "DumpMachine";
    detail: { state: Dump; stateLatex: string | null; qubitCount: number };
  },
) {
  const stateTable = evt.detail.state;
  const qubitCount = evt.detail.qubitCount;
  const basisStates = Object.keys(stateTable);
  const basisColumnWidth = Math.max(
    basisStates[0]?.length ?? 0,
    "Basis".length,
  );
  const basis = "Basis".padEnd(basisColumnWidth);

  let out_str = "";
  out_str += ` ${basis} | Amplitude      | Probability | Phase\n`;
  out_str +=
    " ".padEnd(basisColumnWidth, "-") +
    "-------------------------------------------\n";

  if (qubitCount === 0) {
    out_str += " No qubits allocated";
  } else {
    const rows = [];
    for (const row of basisStates) {
      const [real, imag] = stateTable[row];
      const basis = row.padStart(basisColumnWidth);
      const amplitude = formatComplex(real, imag).padStart(16);
      const probability = formatProbabilityPercent(real, imag).padStart(11);
      const phase = formatPhase(real, imag).padStart(8);

      rows.push(` ${basis} | ${amplitude} | ${probability} | ${phase}`);
    }
    out_str += rows.join("\n");
  }
  return out_str;
}

function formatMatrix(
  evt: Event & {
    type: "Matrix";
    detail: { matrix: number[][][]; matrixLatex: string };
  },
) {
  return evt.detail.matrix
    .map((row) =>
      row.map((entry) => formatComplex(entry[0], entry[1])).join(", "),
    )
    .join("\n");
}

/**
 * Formats a QDK error into a human-readable string with file path, line/column, and message.
 * If a stack trace is available, it formats the full trace; otherwise formats a single-line error.
 */
function formatErrorMessage(error: IQSharpError): string {
  let errorMessage;
  if (error.stack) {
    // The stack trace includes the error message as well, but the
    // URIs need to be parsed out and turned into user-facing file paths
    errorMessage = error.stack
      .split("\n")
      .map((l) => {
        const match = l.match(/^(\s*)at (.*) in (.*):(\d+):(\d+)/);
        if (match) {
          const [, leadingWs, callable, doc, line, column] = match;
          const uri = getSourceUri(doc);
          const displayPath = uri.scheme === "file" ? uri.fsPath : uri;
          return `${leadingWs}at ${callable} in ${displayPath}:${line}:${column}`;
        } else {
          return l;
        }
      })
      .join("\n");
  } else {
    // Format a single-line error message
    const uri = getSourceUri(error.document);
    const displayPath = uri.scheme === "file" ? uri.fsPath : uri;
    const diag = error.diagnostic;
    const location = `${displayPath}:${diag.range.start.line + 1}:${diag.range.start.character + 1}`;
    errorMessage = `${location}: (${diag.code}) ${diag.message}`;
  }
  return errorMessage;
}

function formatErrors(errors: IQSharpError[]): string {
  const errorMessages = [];
  for (const error of errors) {
    const errorMessage = formatErrorMessage(error);
    errorMessages.push(errorMessage);
  }
  return errorMessages.join("\n");
}
