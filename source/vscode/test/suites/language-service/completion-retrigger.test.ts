// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import * as vscode from "vscode";
import {
  activateExtension,
  openDocumentAndWaitForProcessing,
  waitForCondition,
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

  // Roughly a fast typist.
  const keystrokeIntervalMs = 40;

  // Generous, but short enough to fail with a useful message rather than hitting the
  // suite-wide timeout.
  const activeEditorTimeoutMs = 10_000;

  type Invocation = {
    version: number;
    triggerKind: vscode.CompletionTriggerKind;
    triggerCharacter: string | undefined;
  };

  let invocations: Invocation[] = [];
  let recorder: vscode.Disposable | undefined;

  this.beforeAll(async () => {
    await activateExtension();
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

    // `type` goes to whichever editor is active, so typing before this settles sends
    // the keystrokes nowhere and no suggest session is ever opened.
    await waitForCondition(
      () =>
        vscode.window.activeTextEditor?.document.uri.toString() ===
        doc.uri.toString(),
      vscode.window.onDidChangeActiveTextEditor,
      activeEditorTimeoutMs,
      "the document never became the active editor",
    );

    // Land the cursor at the end of the `let foo = "hello!";` line so typed text
    // forms a fresh expression rather than editing existing code.
    const insertAt = new vscode.Position(3, 26);
    editor.selection = new vscode.Selection(insertAt, insertAt);

    const startVersion = doc.version;

    for (const ch of ["S", "t", "d", ".", "D", "i"]) {
      await vscode.commands.executeCommand("type", { text: ch });
      await new Promise((resolve) => setTimeout(resolve, keystrokeIntervalMs));
    }

    // Give any trailing re-triggers a chance to land.
    await new Promise((resolve) => setTimeout(resolve, 1500));

    // Keyed on the trigger kind, which is the only signal specific to the behavior
    // under test. The editor also opens unrelated `Invoke` sessions, sometimes many of
    // them against an unchanging document, and counting those is just machine-speed
    // noise. `TriggerForIncompleteCompletions` means VS Code came back *because* the
    // provider reported the list incomplete.
    const retriggers = invocations.filter(
      (i) =>
        i.triggerKind ===
        vscode.CompletionTriggerKind.TriggerForIncompleteCompletions,
    );

    console.log(
      `qsharp-tests: isIncomplete=${isIncomplete} finalDocVersion=${doc.version}\n` +
        `qsharp-tests:   all invocations: ${invocations
          .map(
            (i) =>
              `v${i.version}/${vscode.CompletionTriggerKind[i.triggerKind]}${i.triggerCharacter ? `('${i.triggerCharacter}')` : ""}`,
          )
          .join(", ")}\n` +
        `qsharp-tests:   incomplete re-triggers: ${retriggers.length}`,
    );

    // Checked before the invocation count so that a failure distinguishes "the
    // keystrokes never arrived" from "they arrived but suggest didn't run".
    assert.isAbove(
      doc.version,
      startVersion,
      "the typed characters never reached the document",
    );
    assert.isNotEmpty(
      invocations,
      "expected the completion provider to be invoked while typing",
    );

    return { retriggers, doc };
  }

  test("a complete list is NOT re-requested on later keystrokes", async () => {
    const { retriggers } = await typeAndRecord(false);

    assert.isEmpty(
      retriggers,
      "expected VS Code to filter a complete list client-side rather than re-requesting",
    );
  });

  test("an incomplete list IS re-requested on later keystrokes", async () => {
    const { retriggers, doc } = await typeAndRecord(true);

    assert.isNotEmpty(
      retriggers,
      "VS Code did not re-invoke the provider after an incomplete list. Returning an " +
        "empty incomplete list is therefore NOT a sufficient mitigation for a " +
        "coalesced-away completion request, and the update loop must avoid coalescing " +
        "past a version a completion request is waiting on.",
    );

    // The point of the mitigation: the request that eventually gets answered is for
    // the version the update loop settles on, not the one that was coalesced away.
    assert.isTrue(
      retriggers.some((i) => i.version === doc.version),
      `expected a re-trigger for the final document version ${doc.version}`,
    );
  });
});
