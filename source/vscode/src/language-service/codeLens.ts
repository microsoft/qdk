// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  ICodeLens,
  ILanguageService,
  qsharpLibraryUriScheme,
} from "qsharp-lang";
import * as vscode from "vscode";
import { isOpenQasmDocument, toVsCodeRange } from "../common";
import { getOpenQasmModeService } from "./openqasmMode.js";

export function createQdkCodeLensProvider(languageService: ILanguageService) {
  return new CodeLensProvider(languageService, mapCodeLens);
}

class CodeLensProvider implements vscode.CodeLensProvider {
  private readonly changedEmitter = new vscode.EventEmitter<void>();
  readonly onDidChangeCodeLenses = this.changedEmitter.event;

  constructor(
    public languageService: ILanguageService,
    private commandMapper: (value: ICodeLens) => vscode.CodeLens,
  ) {
    getOpenQasmModeService()?.onDidResolveMode(() =>
      this.changedEmitter.fire(),
    );
  }
  async provideCodeLenses(
    document: vscode.TextDocument,
  ): Promise<vscode.CodeLens[]> {
    if (document.uri.scheme === qsharpLibraryUriScheme) {
      // Don't show any code lenses for library files, none of the actions
      // would work since compiling library files through the editor is unsupported.
      return [];
    }

    const codeLenses = await this.languageService.getCodeLenses(
      document.uri.toString(),
    );

    if (
      isOpenQasmDocument(document) &&
      (await getOpenQasmModeService()?.getMode(document.uri)) === "spec"
    ) {
      return [
        new vscode.CodeLens(new vscode.Range(0, 0, 0, 0), {
          title: "QDK features are disabled in spec mode. Switch to QDK mode",
          command: "qsharp-vscode.openqasmSwitchToQdk",
        }),
      ];
    }

    return codeLenses.map((cl) => this.commandMapper(cl));
  }
}

function mapCodeLens(cl: ICodeLens): vscode.CodeLens {
  let command;
  let title;
  let tooltip;
  switch (cl.command) {
    case "histogram":
      title = "Histogram";
      command = "qsharp-vscode.showHistogram";
      tooltip = "Run and show histogram";
      break;
    case "estimate":
      title = "Estimate";
      command = "qsharp-vscode.showRe";
      tooltip = "Calculate resource estimates";
      break;
    case "debug":
      title = "Debug";
      command = "qsharp-vscode.debugProgram";
      tooltip = "Debug callable";
      break;
    case "run":
      title = "Run";
      command = "qsharp-vscode.runProgram";
      tooltip = "Run callable";
      break;
    case "circuit":
      title = "Circuit";
      command = "qsharp-vscode.showCircuit";
      tooltip = "Show circuit";
      break;
  }

  return new vscode.CodeLens(toVsCodeRange(cl.range), {
    title,
    command,
    arguments: cl.args ?? [],
    tooltip,
  });
}
