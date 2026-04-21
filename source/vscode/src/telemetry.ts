// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// <reference types="user-agent-data-types" />

import * as vscode from "vscode";
import TelemetryReporter from "@vscode/extension-telemetry";
import { log } from "qsharp-lang";
import { getActiveQdkDocument, getVisibleQdkDocument } from "./programConfig";
import {
  isCircuitDocument,
  isOpenQasmDocument,
  isQdkNotebookCell,
  isQsharpDocument,
  getPlatformEnv,
} from "./common";

export enum EventType {
  InitializePlugin = "Qsharp.InitializePlugin",
  LoadLanguageService = "Qsharp.LoadLanguageService",
  ReturnCompletionList = "Qsharp.ReturnCompletionList",
  GenerateQirStart = "Qsharp.GenerateQirStart",
  GenerateQirEnd = "Qsharp.GenerateQirEnd",
  RenderQuantumStateStart = "Qsharp.RenderQuantumStateStart",
  RenderQuantumStateEnd = "Qsharp.RenderQuantumStateEnd",
  SubmitToAzureStart = "Qsharp.SubmitToAzureStart",
  SubmitToAzureEnd = "Qsharp.SubmitToAzureEnd",
  AuthSessionStart = "Qsharp.AuthSessionStart",
  AuthSessionEnd = "Qsharp.AuthSessionEnd",
  QueryWorkspacesStart = "Qsharp.QueryWorkspacesStart",
  QueryWorkspacesEnd = "Qsharp.QueryWorkspacesEnd",
  AzureRequestFailed = "Qsharp.AzureRequestFailed",
  StorageRequestFailed = "Qsharp.StorageRequestFailed",
  GetJobFilesStart = "Qsharp.GetJobFilesStart",
  GetJobFilesEnd = "Qsharp.GetJobFilesEnd",
  QueryWorkspaceStart = "Qsharp.QueryWorkspaceStart",
  QueryWorkspaceEnd = "Qsharp.QueryWorkspaceEnd",
  CheckCorsStart = "Qsharp.CheckCorsStart",
  CheckCorsEnd = "Qsharp.CheckCorsEnd",
  InitializeRuntimeStart = "Qsharp.InitializeRuntimeStart",
  InitializeRuntimeEnd = "Qsharp.InitializeRuntimeEnd",
  DebugSessionEvent = "Qsharp.DebugSessionEvent",
  Launch = "Qsharp.Launch",
  OpenedDocument = "Qsharp.OpenedDocument",
  TriggerResourceEstimation = "Qsharp.TriggerResourceEstimation",
  ResourceEstimationStart = "Qsharp.ResourceEstimationStart",
  ResourceEstimationEnd = "Qsharp.ResourceEstimationEnd",
  TriggerHistogram = "Qsharp.TriggerHistogram",
  HistogramStart = "Qsharp.HistogramStart",
  NoisySimulation = "Qsharp.NoisySimulation",
  HistogramEnd = "Qsharp.HistogramEnd",
  FormatStart = "Qsharp.FormatStart",
  FormatEnd = "Qsharp.FormatEnd",
  CreateProject = "Qsharp.CreateProject",
  FetchGitHub = "Qsharp.FetchGitHub",
  TriggerCircuit = "Qsharp.TriggerCircuit",
  CircuitStart = "Qsharp.CircuitStart",
  CircuitEnd = "Qsharp.CircuitEnd",
  LanguageModelToolStart = "Qsharp.LanguageModelToolStart",
  LanguageModelToolEnd = "Qsharp.LanguageModelToolEnd",
  RemoveOldCopilotInstructions = "Qsharp.RemoveOldCopilotInstructions",
  ChangelogPromptStart = "Qsharp.ChangelogPromptStart",
  ChangelogPromptEnd = "Qsharp.ChangelogPromptEnd",
  QuantumKatasMcpStart = "Qsharp.QuantumKatasMcpStart",
  KatasPanelAction = "Qsharp.KatasPanelAction",
}

type Empty = { [K in any]: never };

/**
 * Properties of events that are associated with
 * a specific document, e.g. "format" or "open document"
 */
type DocumentEventProperties = {
  documentType: QsharpDocumentType;
};

/**
 * Properties of events that are associated with
 * a user task, e.g. "histogram" or "resource estimation"
 */
type UserTaskProperties = {
  invocationType: UserTaskInvocationType;
};

type EventTypes = {
  [EventType.InitializePlugin]: {
    properties: Empty;
    measurements: Empty;
  };
  [EventType.LoadLanguageService]: {
    properties: Empty;
    measurements: {
      timeToStartMs: number;
    };
  };
  [EventType.ReturnCompletionList]: {
    properties: DocumentEventProperties;
    measurements: { timeToCompletionMs: number; completionListLength: number };
  };
  [EventType.GenerateQirStart]: {
    properties: DocumentEventProperties & {
      associationId: string;
      targetProfile: string;
    };
    measurements: Empty;
  };
  [EventType.GenerateQirEnd]: {
    properties: { associationId: string };
    measurements: { qirLength: number; timeToCompleteMs: number };
  };
  [EventType.RenderQuantumStateStart]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.RenderQuantumStateEnd]: {
    properties: { associationId: string };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.SubmitToAzureStart]: {
    properties: UserTaskProperties & { associationId: string };
    measurements: Empty;
  };
  [EventType.SubmitToAzureEnd]: {
    properties: {
      associationId: string;
      reason?: string;
      flowStatus: UserFlowStatus;
    };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.AuthSessionStart]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.AuthSessionEnd]: {
    properties: {
      associationId: string;
      reason?: string;
      flowStatus: UserFlowStatus;
    };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.QueryWorkspacesStart]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.QueryWorkspacesEnd]: {
    properties: {
      associationId: string;
      reason?: string;
      flowStatus: UserFlowStatus;
    };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.AzureRequestFailed]: {
    properties: { associationId: string; reason?: string };
    measurements: Empty;
  };
  [EventType.StorageRequestFailed]: {
    properties: { associationId: string; reason?: string };
    measurements: Empty;
  };
  [EventType.GetJobFilesStart]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.GetJobFilesEnd]: {
    properties: {
      associationId: string;
      reason?: string;
      flowStatus: UserFlowStatus;
    };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.QueryWorkspaceStart]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.QueryWorkspaceEnd]: {
    properties: {
      associationId: string;
      reason?: string;
      flowStatus: UserFlowStatus;
    };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.CheckCorsStart]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.CheckCorsEnd]: {
    properties: {
      associationId: string;
      reason?: string;
      flowStatus: UserFlowStatus;
    };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.InitializeRuntimeStart]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.InitializeRuntimeEnd]: {
    properties: {
      associationId: string;
      reason?: string;
      flowStatus: UserFlowStatus;
    };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.DebugSessionEvent]: {
    properties: {
      associationId: string;
      event: DebugEvent;
    };
    measurements: Empty;
  };
  [EventType.Launch]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.OpenedDocument]: {
    properties: DocumentEventProperties;
    measurements: { linesOfCode: number };
  };
  [EventType.TriggerResourceEstimation]: {
    properties: DocumentEventProperties &
      UserTaskProperties & {
        associationId: string;
      };
    measurements: Empty;
  };
  [EventType.ResourceEstimationStart]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.ResourceEstimationEnd]: {
    properties: { associationId: string };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.TriggerHistogram]: {
    properties: DocumentEventProperties &
      UserTaskProperties & { associationId: string };
    measurements: Empty;
  };
  [EventType.HistogramStart]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.NoisySimulation]: {
    properties: { associationId: string };
    measurements: Empty;
  };
  [EventType.HistogramEnd]: {
    properties: { associationId: string };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.FormatStart]: {
    properties: DocumentEventProperties & {
      associationId: string;
      event: FormatEvent;
    };
    measurements: Empty;
  };
  [EventType.FormatEnd]: {
    properties: { associationId: string };
    measurements: { timeToCompleteMs: number; numberOfEdits: number };
  };
  [EventType.CreateProject]: {
    properties: Empty;
    measurements: Empty;
  };
  [EventType.FetchGitHub]: {
    properties: { status: string };
    measurements: Empty;
  };
  [EventType.TriggerCircuit]: {
    properties: DocumentEventProperties &
      UserTaskProperties & {
        associationId: string;
      };
    measurements: Empty;
  };
  [EventType.CircuitStart]: {
    properties: {
      associationId: string;
      isOperation: string;
      targetProfile: string;
    };
    measurements: Empty;
  };
  [EventType.CircuitEnd]: {
    properties: {
      simulated: string;
      associationId: string;
      reason?: string;
      flowStatus: UserFlowStatus;
    };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.LanguageModelToolStart]: {
    properties: {
      associationId: string;
      toolName: string;
    };
    measurements: Empty;
  };
  [EventType.LanguageModelToolEnd]: {
    properties: {
      associationId: string;
      reason?: string;
      flowStatus: UserFlowStatus;
    };
    measurements: { timeToCompleteMs: number };
  };
  [EventType.RemoveOldCopilotInstructions]: {
    properties: Empty;
    measurements: Empty;
  };
  [EventType.ChangelogPromptStart]: {
    properties: {
      associationId: string;
      changelogVersion: string;
    };
    measurements: Empty;
  };
  [EventType.ChangelogPromptEnd]: {
    properties: {
      associationId: string;
      action: "showChangelog" | "suppressChangelog";
    };
    measurements: Empty;
  };
  [EventType.QuantumKatasMcpStart]: {
    properties: {
      workspaceRootConfigured: string;
    };
    measurements: Empty;
  };
  [EventType.KatasPanelAction]: {
    properties: {
      action:
        | "refresh"
        | "continue"
        | "openExercise"
        | "openLesson"
        | "askInChat"
        | "setup";
    };
    measurements: Empty;
  };
};

export enum QsharpDocumentType {
  JupyterCell = "JupyterCell",
  Qsharp = "Qsharp",
  Circuit = "Circuit",
  OpenQasm = "OpenQasm",
  Other = "Other",
  Unknown = "Unknown",
}

export enum UserFlowStatus {
  // "Aborted" means the flow was intentionally canceled or left, either by us or the user
  Aborted = "Aborted",
  Succeeded = "Succeeded",
  // "CompletedWithFailure" means something that we can action -- service request failure, exceptions, etc.
  Failed = "Failed",
}

export enum DebugEvent {
  StepIn = "StepIn",
  Continue = "Continue",
}

export enum FormatEvent {
  OnDocument = "OnDocument",
  OnRange = "OnRange",
  OnType = "OnType",
}

export enum UserTaskInvocationType {
  Command = "Command",
  ChatToolCall = "ChatToolCall",
}

let reporter: TelemetryReporter | undefined;
let userAgentString: string | undefined;

export function initTelemetry(context: vscode.ExtensionContext) {
  const packageJson = context.extension?.packageJSON;
  if (!packageJson) {
    return;
  }
  reporter = new TelemetryReporter(packageJson.aiKey);
  const version = context.extension?.packageJSON?.version;
  const browserAndRelease = getBrowserRelease();
  userAgentString = `VSCode/${version} ${browserAndRelease}`;

  sendTelemetryEvent(EventType.InitializePlugin, {}, {});
}

export function sendTelemetryEvent<E extends keyof EventTypes>(
  event: E,
  properties: EventTypes[E]["properties"] = {},
  measurements: EventTypes[E]["measurements"] = {},
) {
  if (reporter === undefined) {
    log.trace(`No telemetry reporter. Omitting telemetry event ${event}`);
    return;
  }

  // If you get a type error here, it's likely because you defined a
  // non-string property or non-number measurement in `EventTypes`.
  // For booleans, use `.toString()` to convert to string and store in `properties`.
  reporter.sendTelemetryEvent(event, properties, measurements);
  log.debug(
    `Sent telemetry: ${event} ${JSON.stringify(properties)} ${JSON.stringify(
      measurements,
    )}`,
  );
}

function getBrowserRelease(): string {
  if (getPlatformEnv() === "node") {
    return `Node/${process.versions.node}`;
  }
  if (navigator.userAgentData?.brands) {
    const browser =
      navigator.userAgentData.brands[navigator.userAgentData.brands.length - 1];
    return `${browser.brand}/${browser.version}`;
  } else {
    return navigator.userAgent;
  }
}

export function getUserAgent(): string {
  if (typeof navigator === "undefined") {
    return userAgentString || `Node/${process.versions.node}`;
  }
  return userAgentString || navigator.userAgent;
}

export function getVisibleDocumentType(): QsharpDocumentType {
  const doc = getVisibleQdkDocument();
  if (!doc) {
    return QsharpDocumentType.Unknown;
  }

  return determineDocumentType(doc);
}

export function getActiveDocumentType(): QsharpDocumentType {
  const doc = getActiveQdkDocument();
  if (!doc) {
    return QsharpDocumentType.Unknown;
  }

  return determineDocumentType(doc);
}

export function determineDocumentType(document: vscode.TextDocument) {
  return isQdkNotebookCell(document)
    ? QsharpDocumentType.JupyterCell
    : isCircuitDocument(document)
      ? QsharpDocumentType.Circuit
      : isQsharpDocument(document)
        ? QsharpDocumentType.Qsharp
        : isOpenQasmDocument(document)
          ? QsharpDocumentType.OpenQasm
          : QsharpDocumentType.Other;
}
