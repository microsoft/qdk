// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import * as vscode from "vscode";
import {
  activateExtension,
  openDocumentAndWaitForProcessing,
} from "../extensionUtils";

/**
 * Measures whether VS Code re-invokes a completion provider as the user keeps typing
 * after an earlier suggest request.
 *
 * This decides how the language service should handle a completion request whose
 * document version gets coalesced away by the update loop. Such a request cannot be
 * answered correctly, since its position refers to text the user has moved past, so it
 * has to return nothing. That is only safe if VS Code comes back and asks again.
 *
 * VS Code re-queries a provider on subsequent keystrokes only when that provider
 * returned `isIncomplete: true`; a complete list is filtered client-side instead.
 * Both cases are measured below, because that difference is exactly what determines
 * whether returning an empty incomplete list is a sufficient mitigation.
 *
 * The recorder is registered as a *second* completion provider. VS Code queries every
 * registered provider within a suggest session and tracks incompleteness per provider,
 * so this observes the real behavior without any production instrumentation.
 *
 * This must drive the editor with the `type` command rather than `editor.edit()` or
 * `vscode.executeCompletionItemProvider`. Only real typing runs the suggest widget's
 * trigger/re-trigger logic, which is the thing being measured.
 */
suite("Completion re-trigger behavior", function suite() {
  const workspaceFolder =
    vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders[0];
  assert(workspaceFolder, "Expecting an open folder");

  const noErrorsQs = vscode.Uri.joinPath(workspaceFolder.uri, "no-errors.qs");

  // Long enough that each keystroke lands while a compile is blocking the extension
  // host, which is the condition that causes updates to coalesce in the first place.
  const simulatedCompileDelayMs = 100;

  // Roughly a fast typist, and deliberately shorter than the simulated compile.
  const keystrokeIntervalMs = 40;

  type Invocation = {
    version: number;
    triggerKind: vscode.CompletionTriggerKind;
    triggerCharacter: string | undefined;
  };

  let invocations: Invocation[] = [];
  let recorder: vscode.Disposable | undefined;

  this.beforeAll(async () => {
    await activateExtension();

    await vscode.workspace
      .getConfiguration("Q#")
      .update(
        "dev.simulatedCompileDelayMs",
        simulatedCompileDelayMs,
        vscode.ConfigurationTarget.Global,
      );
  });

  this.afterAll(async () => {
    await vscode.workspace
      .getConfiguration("Q#")
      .update(
        "dev.simulatedCompileDelayMs",
        undefined,
        vscode.ConfigurationTarget.Global,
      );
  });

  this.afterEach(async () => {
    recorder?.dispose();
    recorder = undefined;
    await vscode.commands.executeCommand(
      "workbench.action.revertAndCloseActiveEditor",
    );
  });

  /**
   * Registers the recording provider, types `Std.Di` one character at a time, and
   * returns the invocations that happened strictly after `.` triggered suggest.
   */
  async function typeAndRecord(isIncomplete: boolean) {
    invocations = [];
    recorder = vscode.languages.registerCompletionItemProvider(
      "qsharp",
      {
        provideCompletionItems(document, _position, _token, context) {
          invocations.push({
            version: document.version,
            triggerKind: context.triggerKind,
            triggerCharacter: context.triggerCharacter,
          });
          // Returns an item rather than an empty list, because VS Code discards an
          // empty result and closes the session, which would suppress re-triggering
          // for reasons unrelated to what is being measured here.
          return new vscode.CompletionList(
            [new vscode.CompletionItem("ZzProbeItem")],
            isIncomplete,
          );
        },
      },
      ".",
    );

    const doc = await openDocumentAndWaitForProcessing(noErrorsQs);
    const editor = await vscode.window.showTextDocument(doc);

    // Land the cursor at the end of the `let foo = "hello!";` line so typed text
    // forms a fresh expression rather than editing existing code.
    const insertAt = new vscode.Position(3, 26);
    editor.selection = new vscode.Selection(insertAt, insertAt);

    let versionAtDot = 0;
    for (const ch of ["S", "t", "d", ".", "D", "i"]) {
      await vscode.commands.executeCommand("type", { text: ch });
      if (ch === ".") {
        versionAtDot = doc.version;
      }
      await new Promise((resolve) => setTimeout(resolve, keystrokeIntervalMs));
    }

    // Give any trailing re-triggers a chance to land.
    await new Promise((resolve) => setTimeout(resolve, 1500));

    // Keyed on the document version rather than the trigger kind: when a suggest
    // session is already open, VS Code may re-query an incomplete provider instead of
    // starting a fresh trigger-character session, so the kind isn't dependable.
    const afterDot = invocations.filter((i) => i.version > versionAtDot);

    console.log(
      `qsharp-tests: isIncomplete=${isIncomplete} versionAtDot=${versionAtDot} finalDocVersion=${doc.version}\n` +
        `qsharp-tests:   all invocations: ${invocations
          .map(
            (i) =>
              `v${i.version}/${vscode.CompletionTriggerKind[i.triggerKind]}${i.triggerCharacter ? `('${i.triggerCharacter}')` : ""}`,
          )
          .join(", ")}\n` +
        `qsharp-tests:   invocations after dot: ${afterDot.length}`,
    );

    assert.isNotEmpty(
      invocations,
      "expected the completion provider to be invoked while typing",
    );
    assert.isAbove(
      doc.version,
      versionAtDot,
      "expected more edits after the `.` keystroke",
    );

    return { versionAtDot, afterDot, doc };
  }

  test("a complete list is NOT re-requested on later keystrokes", async () => {
    const { afterDot } = await typeAndRecord(false);

    assert.isEmpty(
      afterDot,
      "expected VS Code to filter a complete list client-side rather than re-requesting",
    );
  });

  test("an incomplete list IS re-requested on later keystrokes", async () => {
    const { afterDot } = await typeAndRecord(true);

    assert.isNotEmpty(
      afterDot,
      "VS Code did not re-invoke the provider after an incomplete list. Returning an " +
        "empty incomplete list is therefore NOT a sufficient mitigation for a " +
        "coalesced-away completion request, and the update loop must avoid coalescing " +
        "past a version a completion request is waiting on (plan Phase 3b).",
    );
  });
});
