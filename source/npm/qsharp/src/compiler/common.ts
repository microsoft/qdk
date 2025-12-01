// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { IQSharpError, type VSDiagnostic } from "../../lib/web/qsc_wasm.js";

// Each DumpMachine output is represented as an object where each key is a basis
// state, e.g., "|3>" and the value is the [real, imag] parts of the complex amplitude.
export type Dump = {
  [index: string]: [number, number];
};

export type Result =
  | { success: true; value: string }
  | {
      success: false;
      /**
       * The `VSDiagnostic` variant is deprecated and kept for backward compatibility.
       * Check type at runtime and prefer `IQSharpError[]` when possible, as it
       * contains all the information in `VSDiagnostic` plus additional fields.
       */
      value: VSDiagnostic & { errors: IQSharpError[] };
    };

interface DumpMsg {
  type: "DumpMachine";
  state: Dump;
  stateLatex: string | null;
  qubitCount: number;
}

interface MatrixMsg {
  type: "Matrix";
  matrix: number[][][]; // Array or rows, which are an array of elements, which are complex numbers as [re, im]
  matrixLatex: string;
}

interface MessageMsg {
  type: "Message";
  message: string;
}

interface ResultMsg {
  type: "Result";
  result: Result;
}

type EventMsg = ResultMsg | DumpMsg | MatrixMsg | MessageMsg;

function outputAsResult(msg: string): ResultMsg | null {
  try {
    const obj = JSON.parse(msg);
    if (obj?.type == "Result" && typeof obj.success == "boolean") {
      return {
        type: "Result",
        result: {
          success: obj.success,
          value: obj.result,
        },
      };
    }
  } catch {
    return null;
  }
  return null;
}

function outputAsMessage(msg: string): MessageMsg | null {
  try {
    const obj = JSON.parse(msg);
    if (obj?.type == "Message" && typeof obj.message == "string") {
      return obj as MessageMsg;
    }
  } catch {
    return null;
  }
  return null;
}

function outputAsDump(msg: string): DumpMsg | null {
  try {
    const obj = JSON.parse(msg);
    if (obj?.type == "DumpMachine" && typeof obj.state == "object") {
      return obj as DumpMsg;
    }
  } catch {
    return null;
  }
  return null;
}

function outputAsMatrix(msg: string): MatrixMsg | null {
  try {
    const obj = JSON.parse(msg);
    if (obj?.type == "Matrix" && Array.isArray(obj.matrix)) {
      return obj as MatrixMsg;
    }
  } catch {
    return null;
  }
  return null;
}

export function eventStringToMsg(msg: string): EventMsg | null {
  return (
    outputAsResult(msg) ||
    outputAsMessage(msg) ||
    outputAsDump(msg) ||
    outputAsMatrix(msg)
  );
}

export type ShotResult = {
  success: boolean;
  /**
   * `VSDiagnostic` is kept for backward compatibility, check runtime type
   * for `IQSharpError[]` and prefer that when possible.
   */
  result: string | (VSDiagnostic & { errors: IQSharpError[] });
  events: Array<MessageMsg | DumpMsg | MatrixMsg>;
};
