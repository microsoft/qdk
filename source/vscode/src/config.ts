// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log, MAX_CLIFFORD_QUBITS } from "qsharp-lang";
import * as vscode from "vscode";
import type { SimulatorConfig } from "qsharp-lang";

export function getTargetFriendlyName(targetProfile?: string) {
  switch (targetProfile) {
    case "base":
      return "QIR Base";
    case "adaptive_ri":
      return "QIR Adaptive RI";
    case "adaptive_rif":
      return "QIR Adaptive RIF";
    case "adaptive":
      return "QIR Adaptive";
    case "unrestricted":
      return "QIR Unrestricted";
    default:
      log.error("invalid target profile found: {}", targetProfile);
      return "QIR invalid";
  }
}

export function getPauliNoiseModel(): number[] {
  const pauliNoiseSettings = vscode.workspace.getConfiguration(
    "Q#.simulation.pauliNoise",
  );
  const noiseTuple = [
    pauliNoiseSettings.get("X", 0),
    pauliNoiseSettings.get("Y", 0),
    pauliNoiseSettings.get("Z", 0),
  ];
  return noiseTuple;
}

export function getQubitLossSetting(): number {
  const qubitLoss = vscode.workspace
    .getConfiguration("Q#.simulation")
    .get<number>("qubitLoss", 0);
  return qubitLoss;
}

export function getSimulationConfig(): SimulatorConfig {
  const simulationSettings = vscode.workspace.getConfiguration("Q#.simulation");
  const type = simulationSettings.get<string>("type", "sparse");
  if (type === "sparse") {
    return { type };
  }
  if (type !== "clifford") {
    throw new Error(
      `Invalid Q#.simulation.type value "${type}". Expected "sparse" or "clifford".`,
    );
  }

  const maxQubits = simulationSettings.get<number>("clifford.maxQubits", 1000);
  if (
    !Number.isSafeInteger(maxQubits) ||
    maxQubits < 1 ||
    maxQubits > MAX_CLIFFORD_QUBITS
  ) {
    throw new Error(
      `Q#.simulation.clifford.maxQubits must be an integer between 1 and ${MAX_CLIFFORD_QUBITS}.`,
    );
  }
  return { type, maxQubits };
}

export function validateSimulationNoiseSettings(
  simulation: SimulatorConfig,
  qubitLoss: number,
): void {
  if (simulation.type === "clifford" && qubitLoss !== 0) {
    throw new Error(
      "Q#.simulation.qubitLoss is not supported by the Clifford simulator. Set it to zero or select the sparse simulator.",
    );
  }
}

export function getShowDevDiagnostics(): boolean {
  return vscode.workspace
    .getConfiguration("Q#")
    .get<boolean>("dev.showDevDiagnostics", false);
}

export function getUploadSupplementalData(): boolean {
  return vscode.workspace
    .getConfiguration("Q#")
    .get<boolean>("azure.uploadSupplementalData", true);
}

export function getTargetJobParams(targetId: string): Record<string, unknown> {
  const raw = vscode.workspace
    .getConfiguration("Q#")
    .get<Record<string, Record<string, unknown>>>("azure.targetJobParams", {});
  // Deep clone the entire setting to materialize VS Code's configuration proxy
  // into a plain object. Without this, nested values may not survive object
  // spread operations.
  const allTargetParams: Record<string, Record<string, unknown>> = JSON.parse(
    JSON.stringify(raw),
  );
  return allTargetParams[targetId] ?? {};
}
