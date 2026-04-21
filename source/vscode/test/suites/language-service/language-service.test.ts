// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import * as vscode from "vscode";
import {
  activateExtension,
  openDocumentAndWaitForProcessing,
  waitForDiagnosticsToAppear,
  waitForDiagnosticsToBeEmpty,
} from "../extensionUtils";

suite("Q# Language Service Tests", function suite() {
  const workspaceFolder =
    vscode.workspace.workspaceFolders && vscode.workspace.workspaceFolders[0];
  assert(workspaceFolder, "Expecting an open folder");

  const workspaceFolderUri = workspaceFolder.uri;
  const joinPath = vscode.Uri.joinPath;
  const packages = joinPath(workspaceFolderUri, "packages");

  const testQs = joinPath(workspaceFolderUri, "test.qs");
  const noErrorsQs = joinPath(workspaceFolderUri, "no-errors.qs");
  const mainPackageMainQs = joinPath(packages, "MainPackage", "src", "Main.qs");
  const depPackageMainQs = joinPath(packages, "DepPackage", "src", "Main.qs");
  const missingDepMainQs = joinPath(packages, "MissingDep", "src", "Main.qs");
  const missingDepManifest = joinPath(packages, "MissingDep", "qsharp.json");
  const badManifestMainQs = joinPath(packages, "BadManifest", "src", "Main.qs");
  const badManifestManifest = joinPath(packages, "BadManifest", "qsharp.json");
  const circularDepMainQs = joinPath(packages, "CircularDep", "src", "Main.qs");
  const circularDepManifest = joinPath(packages, "CircularDep", "qsharp.json");
  const hasBadDepMainQs = joinPath(packages, "HasBadDep", "src", "Main.qs");
  const withSyntaxErrorMainQs = joinPath(
    packages,
    "WithSyntaxError",
    "src",
    "Main.qs",
  );

  this.beforeAll(async () => {
    await activateExtension();

    // Bring up Problems view for when we want to visually inspect what's going on
    vscode.commands.executeCommand("workbench.action.problems.focus");
  });

  test("Q# language is registered", async () => {
    const doc = await openDocumentAndWaitForProcessing(testQs);
    assert.equal(
      doc.languageId,
      "qsharp",
      "document language should be `qsharp`",
    );
  });

  test("Completions", async () => {
    await openDocumentAndWaitForProcessing(testQs);
    const actualCompletionList = (await vscode.commands.executeCommand(
      "vscode.executeCompletionItemProvider",
      testQs,
      new vscode.Position(1, 0),
    )) as vscode.CompletionList;

    assert.include(
      actualCompletionList.items.map((i) => i.label),
      "operation",
    );

    assert.include(
      actualCompletionList.items.map((i) => i.label),
      "Shor sample",
    );
  });

  test("Completions - don't include samples when syntactically inappropriate", async () => {
    await openDocumentAndWaitForProcessing(testQs);
    const actualCompletionList = (await vscode.commands.executeCommand(
      "vscode.executeCompletionItemProvider",
      testQs,
      new vscode.Position(12, 0), // put the cursor after the namespace declaration
    )) as vscode.CompletionList;

    assert.notInclude(
      actualCompletionList.items.map((i) => i.label),
      "Shor sample",
    );
  });

  test("Definition", async () => {
    const doc = await openDocumentAndWaitForProcessing(testQs);
    const text = doc.getText(
      new vscode.Range(new vscode.Position(4, 16), new vscode.Position(4, 19)),
    );
    // Sanity check the test setup - is this the correct position?
    assert.equal(text, "foo");

    const actualDefinition = (await vscode.commands.executeCommand(
      "vscode.executeDefinitionProvider",
      testQs,
      new vscode.Position(4, 18), // cursor on the usage of foo
    )) as vscode.Location[];

    const location = actualDefinition[0];
    assert.equal(location.uri.toString(), testQs.toString());
    assert.equal(location.range.start.line, 3);
    assert.equal(location.range.start.character, 12);
  });

  test("Diagnostics", async () => {
    await openDocumentAndWaitForProcessing(testQs);
    const actualDiagnostics = await waitForDiagnosticsToAppear(testQs);
    assert.lengthOf(actualDiagnostics, 1);

    assert.include(actualDiagnostics[0].message, "syntax error");
    assert.equal(actualDiagnostics[0].range.start.line, 7);
  });

  test("Hover", async () => {
    const doc = await openDocumentAndWaitForProcessing(testQs);
    const text = doc.getText(
      new vscode.Range(new vscode.Position(4, 16), new vscode.Position(4, 19)),
    );
    // Sanity check the test setup - is this the correct position?
    assert.equal(text, "foo");

    const actualHovers = (await vscode.commands.executeCommand(
      "vscode.executeHoverProvider",
      testQs,
      new vscode.Position(4, 18), // cursor on the usage of foo
    )) as vscode.Hover[];

    assert.lengthOf(actualHovers, 1);
    assert.lengthOf(actualHovers[0].contents, 1);
    const md = actualHovers[0].contents[0] as vscode.MarkdownString;
    assert.include(md.value, "foo : String");
  });

  test("Signature Help", async () => {
    const doc = await openDocumentAndWaitForProcessing(testQs);
    const text = doc.getText(
      new vscode.Range(new vscode.Position(4, 16), new vscode.Position(4, 19)),
    );
    // Sanity check the test setup - is this the correct position?
    assert.equal(text, "foo");

    const actualSignatureHelp = (await vscode.commands.executeCommand(
      "vscode.executeSignatureHelpProvider",
      testQs,
      new vscode.Position(4, 18), // cursor on the usage of foo
    )) as vscode.SignatureHelp;

    assert.lengthOf(actualSignatureHelp.signatures, 1);
    assert.include(
      actualSignatureHelp.signatures[0].label,
      "function Message(msg : String)",
    );
  });

  test("Format Document", async () => {
    await openDocumentAndWaitForProcessing(testQs);

    const actualFormatEdits = (await vscode.commands.executeCommand(
      "vscode.executeFormatDocumentProvider",
      testQs,
    )) as vscode.TextEdit[];

    assert.lengthOf(actualFormatEdits, 1);
    assert.equal(actualFormatEdits[0].range.start.line, 7);
    assert.equal(actualFormatEdits[0].range.start.character, 27);
    assert.equal(actualFormatEdits[0].range.end.line, 8);
    assert.equal(actualFormatEdits[0].range.end.character, 4);
    assert.equal(actualFormatEdits[0].newText, "");
  });

  test("Format Document Range", async () => {
    await openDocumentAndWaitForProcessing(testQs);

    const noEditRange = new vscode.Range(
      new vscode.Position(7, 24),
      new vscode.Position(7, 27),
    );
    const editRange = new vscode.Range(
      new vscode.Position(7, 27),
      new vscode.Position(8, 4),
    );

    let actualFormatEdits = (await vscode.commands.executeCommand(
      "vscode.executeFormatRangeProvider",
      testQs,
      noEditRange,
    )) as vscode.TextEdit[];

    // assert that edits outside the range aren't returned
    assert.isUndefined(actualFormatEdits);

    actualFormatEdits = (await vscode.commands.executeCommand(
      "vscode.executeFormatRangeProvider",
      testQs,
      editRange,
    )) as vscode.TextEdit[];

    assert.lengthOf(actualFormatEdits, 1);
    assert.equal(actualFormatEdits[0].range.start.line, 7);
    assert.equal(actualFormatEdits[0].range.start.character, 27);
    assert.equal(actualFormatEdits[0].range.end.line, 8);
    assert.equal(actualFormatEdits[0].range.end.character, 4);
    assert.equal(actualFormatEdits[0].newText, "");
  });

  test("Code Lens", async () => {
    const doc = await openDocumentAndWaitForProcessing(noErrorsQs);

    const actualCodeLenses = (await vscode.commands.executeCommand(
      "vscode.executeCodeLensProvider",
      doc.uri,
    )) as vscode.CodeLens[];

    assert.lengthOf(actualCodeLenses, 4);

    for (const lens of actualCodeLenses) {
      assert.include(doc.getText(lens.range), "function Test()");
    }
  });

  test("Package dependencies", async () => {
    const doc = await openDocumentAndWaitForProcessing(mainPackageMainQs);

    // No errors if package dependencies are properly resolved
    await waitForDiagnosticsToBeEmpty(mainPackageMainQs);

    // Sanity check the test setup - is this the correct position?
    const text = doc.getText(
      new vscode.Range(new vscode.Position(1, 4), new vscode.Position(1, 26)),
    );
    assert.equal(
      text,
      "MyDep.MyDep.MyFunction",
      `${mainPackageMainQs.fsPath} file contents don't match expected`,
    );

    // Verify go-to-definition works across packages
    const actualDefinition = (await vscode.commands.executeCommand(
      "vscode.executeDefinitionProvider",
      mainPackageMainQs,
      new vscode.Position(1, 20), // cursor on the usage of "MyFunction"
    )) as vscode.Location[];

    // Returned location should be in DepPackage on the definition of "MyFunction"
    assert.lengthOf(
      actualDefinition,
      1,
      "Expected to find one definition for MyFunction",
    );
    const location = actualDefinition[0];
    assert.equal(location.uri.toString(), depPackageMainQs.toString());
    assert.equal(location.range.start.line, 1);
    assert.equal(location.range.start.character, 13);
  });

  test("Web package dependencies", async () => {
    const doc = await openDocumentAndWaitForProcessing(mainPackageMainQs);

    // Sanity check the test setup - is this the correct position?
    const text = doc.getText(
      new vscode.Range(new vscode.Position(2, 4), new vscode.Position(2, 32)),
    );
    assert.equal(text, "GitHubDep.Library.MyFunction");

    // Verify go-to-definition works across packages
    const actualDefinition = (await vscode.commands.executeCommand(
      "vscode.executeDefinitionProvider",
      mainPackageMainQs,
      new vscode.Position(2, 30), // cursor on the usage of "MyFunction"
    )) as vscode.Location[];

    // Returned location should be in the web dependency on the definition of "MyFunction"
    assert.lengthOf(
      actualDefinition,
      1,
      "Expected to find one definition for MyFunction",
    );
    const location = actualDefinition[0];
    assert.equal(
      location.uri.toString(),
      "qsharp-github-source:test-owner/test-repo/test-ref/src/Main.qs",
    );
    assert.equal(location.range.start.line, 1);
    assert.equal(location.range.start.character, 13);

    // No errors if package dependencies are properly resolved
    await waitForDiagnosticsToBeEmpty(mainPackageMainQs);
  });

  test("Manifest errors should be reported", async () => {
    // Can't parse qsharp.json
    await openDocumentAndWaitForProcessing(badManifestMainQs);
    const actualDiagnostics =
      await waitForDiagnosticsToAppear(badManifestManifest);
    assert.lengthOf(
      actualDiagnostics,
      1,
      `Expected errors for ${badManifestManifest.fsPath}`,
    );
    assert.include(
      actualDiagnostics[0].message,
      "Failed to parse manifest",
      `Expected syntax error in ${badManifestManifest.fsPath}`,
    );
  });

  test("Package resolution errors should be reported", async () => {
    // Dependency missing
    await openDocumentAndWaitForProcessing(missingDepMainQs);
    let actualDiagnostics =
      await waitForDiagnosticsToAppear(missingDepManifest);
    assert.lengthOf(
      actualDiagnostics,
      1,
      `Expected errors for ${missingDepManifest.fsPath}`,
    );
    assert.include(
      actualDiagnostics[0].message,
      "File system error",
      `Expected file system error in ${missingDepManifest.fsPath}`,
    );

    // Circular dependency
    await openDocumentAndWaitForProcessing(circularDepMainQs);
    actualDiagnostics = await waitForDiagnosticsToAppear(circularDepManifest);
    assert.lengthOf(
      actualDiagnostics,
      1,
      `Expected errors for ${circularDepManifest.fsPath}`,
    );
    assert.include(
      actualDiagnostics[0].message,
      "Circular dependency detected",
      `Expected circular dependency error in ${circularDepManifest.fsPath}`,
    );
  });

  test("Errors from dependencies should be reported", async () => {
    // Dependency has syntax error

    // For a meaningful test, DON'T open the dependency source.
    // Ensure dependency errors are still reported if the parent package is built.
    await openDocumentAndWaitForProcessing(hasBadDepMainQs);

    const actualDiagnostics = await waitForDiagnosticsToAppear(
      withSyntaxErrorMainQs,
    );
    assert.lengthOf(
      actualDiagnostics,
      1,
      `Expected errors for ${withSyntaxErrorMainQs.fsPath}`,
    );
    assert.include(
      actualDiagnostics[0].message,
      "syntax error",
      `Expected syntax error in ${withSyntaxErrorMainQs.fsPath}`,
    );
  });
});
