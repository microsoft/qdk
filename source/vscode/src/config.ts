// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";

export function getTargetFriendlyName(targetProfile?: string) {
  switch (targetProfile) {
    case "base":
      return "QIR Base";
    case "adaptive_ri":
      return "QIR Adaptive RI";
    case "adaptive_rif":
      return "QIR Adaptive RIF";
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

export function getShowDevDiagnostics(): boolean {
  return vscode.workspace
    .getConfiguration("Q#")
    .get<boolean>("dev.showDevDiagnostics", false);
}

export function getUploadSupplementalData(): boolean {
  return vscode.workspace
    .getConfiguration("Q#")
    .get<boolean>("azure.experimental.uploadSupplementalData", false);
}
