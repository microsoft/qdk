// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  ILanguageService,
  LanguageServiceModeResolvedEvent,
} from "qsharp-lang";
import * as vscode from "vscode";
import { isOpenQasmDocument } from "../common.js";

export type EffectiveOpenQasmMode = "qdk" | "spec";

export class OpenQasmModeService implements vscode.Disposable {
  private readonly modes = new Map<string, EffectiveOpenQasmMode>();
  private readonly resolvedEmitter = new vscode.EventEmitter<{
    uri: string;
    mode: EffectiveOpenQasmMode;
  }>();
  private readonly listener = (event: LanguageServiceModeResolvedEvent) => {
    this.modes.set(event.detail.uri, event.detail.mode);
    this.resolvedEmitter.fire(event.detail);
  };

  readonly onDidResolveMode = this.resolvedEmitter.event;

  constructor(private readonly languageService: ILanguageService) {
    languageService.addEventListener("modeResolved", this.listener);
  }

  async getMode(uri: vscode.Uri): Promise<EffectiveOpenQasmMode | undefined> {
    const uriString = uri.toString();
    const mode = await this.languageService.getOpenQasmMode(uriString);
    if (mode) {
      this.modes.set(uriString, mode);
    }
    return mode ?? this.modes.get(uriString);
  }

  async setOverride(
    uri: vscode.Uri,
    mode: EffectiveOpenQasmMode | undefined,
  ): Promise<void> {
    await this.languageService.setOpenQasmModeOverride(uri.toString(), mode);
  }

  async awaitFirstResolution(
    uri: vscode.Uri,
    timeoutMs = 1_000,
  ): Promise<EffectiveOpenQasmMode | undefined> {
    const known = await this.getMode(uri);
    if (known) {
      return known;
    }

    const uriString = uri.toString();
    const cached = this.modes.get(uriString);
    if (cached) {
      return cached;
    }

    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        subscription.dispose();
        resolve(undefined);
      }, timeoutMs);
      const subscription = this.onDidResolveMode((event) => {
        if (event.uri === uriString) {
          clearTimeout(timer);
          subscription.dispose();
          resolve(event.mode);
        }
      });
    });
  }

  async awaitMode(
    uri: vscode.Uri,
    mode: EffectiveOpenQasmMode,
    timeoutMs = 1_000,
  ): Promise<boolean> {
    const uriString = uri.toString();

    // Fast path for already-resolved state, including queued language-service updates.
    const cached = this.modes.get(uriString);
    if (cached === mode) {
      return true;
    }

    const known = await this.getMode(uri);
    if (known === mode) {
      return true;
    }

    return new Promise((resolve) => {
      let settled = false;
      const subscription = this.onDidResolveMode((event) => {
        if (event.uri === uriString && event.mode === mode) {
          finish(true);
        }
      });

      const timer = setTimeout(() => {
        finish(false);
      }, timeoutMs);

      const finish = (value: boolean) => {
        if (settled) {
          return;
        }
        settled = true;
        clearTimeout(timer);
        subscription.dispose();
        resolve(value);
      };

      // Close the check/subscribe race: if mode resolved between the checks above
      // and listener registration, treat it as success immediately.
      if (this.modes.get(uriString) === mode) {
        finish(true);
      }
    });
  }

  dispose(): void {
    this.languageService.removeEventListener("modeResolved", this.listener);
    this.resolvedEmitter.dispose();
  }
}

let service: OpenQasmModeService | undefined;

export function initializeOpenQasmModeService(
  languageService: ILanguageService,
): OpenQasmModeService {
  service?.dispose();
  service = new OpenQasmModeService(languageService);
  return service;
}

export function getOpenQasmModeService(): OpenQasmModeService | undefined {
  return service;
}

const modeContextKey = "qsharp-vscode.openqasmMode";

async function updateModeContext() {
  const document = vscode.window.activeTextEditor?.document;
  const mode =
    document && isOpenQasmDocument(document)
      ? await service?.getMode(document.uri)
      : undefined;
  await vscode.commands.executeCommand("setContext", modeContextKey, mode);
}

export function registerOpenQasmModeCommands(): vscode.Disposable[] {
  const updateContext = () => void updateModeContext();
  const switchMode = async (mode: EffectiveOpenQasmMode | undefined) => {
    const document = vscode.window.activeTextEditor?.document;
    if (!document || !isOpenQasmDocument(document)) {
      return;
    }
    await service?.setOverride(document.uri, mode);
  };

  const subscriptions = [
    vscode.commands.registerCommand("qsharp-vscode.openqasmSwitchToQdk", () =>
      switchMode("qdk"),
    ),
    vscode.commands.registerCommand("qsharp-vscode.openqasmSwitchToSpec", () =>
      switchMode("spec"),
    ),
    vscode.commands.registerCommand("qsharp-vscode.openqasmResetMode", () =>
      switchMode(undefined),
    ),
    vscode.window.onDidChangeActiveTextEditor(updateContext),
    service?.onDidResolveMode(updateContext),
  ].filter((subscription): subscription is vscode.Disposable => !!subscription);

  updateContext();
  return subscriptions;
}

export async function ensureQdkFeaturesAvailable(
  document: vscode.TextDocument,
  resumeAfterSwitch = false,
): Promise<string | undefined> {
  if (!isOpenQasmDocument(document)) {
    return undefined;
  }

  const mode = await service?.awaitFirstResolution(document.uri);
  if (mode === "qdk") {
    return undefined;
  }

  const selection = await vscode.window.showInformationMessage(
    mode === "spec"
      ? "QDK features are unavailable while this OpenQASM file is in spec mode."
      : "OpenQASM mode has not resolved yet. Try again once the file finishes loading.",
    "Switch to QDK mode",
  );
  if (selection === "Switch to QDK mode" && mode === "spec") {
    await service?.setOverride(document.uri, "qdk");
    if (resumeAfterSwitch && (await service?.awaitMode(document.uri, "qdk"))) {
      return undefined;
    }
  }

  return mode === "spec"
    ? "QDK features are unavailable while this OpenQASM file is in spec mode."
    : "OpenQASM mode has not resolved yet. Try again once the file finishes loading.";
}
