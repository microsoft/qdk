// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { ILanguageService, samples, openqasm_samples } from "qsharp-lang";
import * as vscode from "vscode";
import { CompletionItem } from "vscode";
import { isOpenQasmDocument, isQsharpDocument, toVsCodeRange } from "../common";
import {
  determineDocumentType,
  EventType,
  sendTelemetryEvent,
} from "../telemetry";

export function createCompletionItemProvider(
  languageService: ILanguageService,
) {
  return new QSharpCompletionItemProvider(languageService);
}

class QSharpCompletionItemProvider implements vscode.CompletionItemProvider {
  private samples: vscode.CompletionItem[] = [];
  private openqasm_samples: vscode.CompletionItem[] = [];

  constructor(public languageService: ILanguageService) {
    this.samples = samples.map((s) => {
      const item = new CompletionItem(
        s.title + " sample",
        vscode.CompletionItemKind.Snippet,
      );
      item.insertText = s.code;
      return item;
    });
    this.openqasm_samples = openqasm_samples.map((s) => {
      const item = new CompletionItem(
        s.title + " sample",
        vscode.CompletionItemKind.Snippet,
      );
      item.insertText = s.code;
      return item;
    });
  }

  async provideCompletionItems(
    document: vscode.TextDocument,
    position: vscode.Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    token: vscode.CancellationToken,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    context: vscode.CompletionContext,
  ) {
    const start = performance.now();
    const completions = await this.languageService.getCompletions(
      document.uri.toString(),
      document.version,
      position,
    );
    const end = performance.now();
    sendTelemetryEvent(
      EventType.ReturnCompletionList,
      {
        documentType: determineDocumentType(document),
      },
      {
        timeToCompletionMs: end - start,
        completionListLength: completions.items.length,
      },
    );
    let results = completions.items.map((c) => {
      let kind;
      switch (c.kind) {
        case "function":
          kind = vscode.CompletionItemKind.Function;
          break;
        case "interface":
          kind = vscode.CompletionItemKind.Interface;
          break;
        case "keyword":
          kind = vscode.CompletionItemKind.Keyword;
          break;
        case "variable":
          kind = vscode.CompletionItemKind.Variable;
          break;
        case "typeParameter":
          kind = vscode.CompletionItemKind.TypeParameter;
          break;
        case "module":
          kind = vscode.CompletionItemKind.Module;
          break;
        case "property":
          kind = vscode.CompletionItemKind.Property;
          break;
        case "field":
          kind = vscode.CompletionItemKind.Field;
          break;
        case "class":
          kind = vscode.CompletionItemKind.Class;
          break;
      }
      const item = new CompletionItem(c.label, kind);
      item.sortText = c.sortText;
      item.detail = c.detail;
      item.additionalTextEdits = c.additionalTextEdits?.map((edit) => {
        return new vscode.TextEdit(toVsCodeRange(edit.range), edit.newText);
      });
      return item;
    });

    // In qsharp documents include the qsharp samples in contexts that are syntactically
    // appropriate. The presence of the "operation" keyword in the completion list
    // is a hint that the cursor is at a point we can insert the sample code.
    if (
      isQsharpDocument(document) &&
      results.findIndex(
        (i) =>
          i.kind === vscode.CompletionItemKind.Keyword &&
          i.label === "operation",
      ) !== -1
    ) {
      results = results.concat(this.samples);
    }

    // In OpenQASM documents include the OpenQASM samples in contexts that are syntactically
    // appropriate. The presence of the "OPENQASM" keyword in the completion list
    // is a hint that the cursor is at a point we can insert the sample code.
    if (
      isOpenQasmDocument(document) &&
      results.findIndex(
        (i) =>
          i.kind === vscode.CompletionItemKind.Keyword &&
          i.label === "OPENQASM",
      ) !== -1
    ) {
      results = results.concat(this.openqasm_samples);
    }

    // Preserves `isIncomplete`, which is what makes VS Code ask again on the next
    // keystroke instead of filtering this list client-side.
    return new vscode.CompletionList(
      results,
      completions.isIncomplete === true,
    );
  }
}
