// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  ICodeLens,
  ILanguageService,
  qsharpLibraryUriScheme,
} from "qsharp-lang";
import * as vscode from "vscode";
import { toVsCodeRange } from "../common";

export function createQdkCodeLensProvider(languageService: ILanguageService) {
  return new CodeLensProvider(languageService, mapCodeLens);
}

class CodeLensProvider implements vscode.CodeLensProvider {
  constructor(
    public languageService: ILanguageService,
    private commandMapper: (value: ICodeLens) => vscode.CodeLens,
  ) {}
  // We could raise events when code lenses change,
  // but there's no need as the editor seems to query often enough.
  // onDidChangeCodeLenses?: vscode.Event<void> | undefined;
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
    case "flamegraph":
      title = "Flamegraph";
      command = "qsharp-vscode.showFlamegraph";
      tooltip = "Show flamegraph";
      break;
  }

  return new vscode.CodeLens(toVsCodeRange(cl.range), {
    title,
    command,
    arguments: cl.args ?? [],
    tooltip,
  });
}
