// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { ILanguageService } from "qsharp";
import * as vscode from "vscode";
import { CompletionItem } from "vscode";

export function createCompletionItemProvider(
  languageService: ILanguageService
) {
  return new QSharpCompletionItemProvider(languageService);
}

class QSharpCompletionItemProvider implements vscode.CompletionItemProvider {
  constructor(public languageService: ILanguageService) {}

  async provideCompletionItems(
    document: vscode.TextDocument,
    position: vscode.Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    token: vscode.CancellationToken,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    context: vscode.CompletionContext
  ) {
    const completions = await this.languageService.getCompletions(
      document.uri.toString(),
      document.offsetAt(position)
    );
    return completions.items.map((c) => {
      let kind;
      switch (c.kind) {
        case "function":
          kind = vscode.CompletionItemKind.Function;
          break;
        case "module":
          kind = vscode.CompletionItemKind.Module;
          break;
        case "keyword":
          kind = vscode.CompletionItemKind.Keyword;
          break;
        case "issue":
          kind = vscode.CompletionItemKind.Issue;
          break;
      }
      return new CompletionItem(c.label, kind);
    });
  }
}
