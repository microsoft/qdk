// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { type ExtensionApi } from "../../src/extension";

// Q# extension log level. Increase this for debugging.
const extensionLogLevel = "warn";

// The code for the document status diagnostic.
const documentStatusDiagnosticCode = "Qdk.Dev.DocumentStatus";

let testGithubEndpoint = "";
export function setTestGithubEndpoint(url: string) {
  // In the web environment, http://localhost:3000/static/mount is set up by
  // @vscode/test-web to serve the test workspace files. We use a subfolder
  // as a fake GitHub raw content endpoint.
  //
  // In the node environment, the language-service test suite starts its own
  // HTTP server and passes the endpoint URL here, where it is stored in the
  // module-level testGithubEndpoint variable for use during activation.
  testGithubEndpoint = url;
}

export async function activateExtension() {
  // Check for pre-release or stable builds of the extension, as could be in release pipeline
  const ext =
    vscode.extensions.getExtension("quantum.qsharp-lang-vscode-dev") ??
    vscode.extensions.getExtension("quantum.qsharp-lang-vscode");

  if (!ext) {
    throw new Error("qsharp extension not found");
  }

  if (ext.isActive) {
    return;
  }

  const start = performance.now();
  const extensionApi: ExtensionApi = await ext.activate();

  extensionApi.setGithubEndpoint(testGithubEndpoint);

  const logForwarder = extensionApi.logging;
  if (!logForwarder) {
    throw new Error(`qsharp-tests: extension did not return a log forwarder`);
  }

  logForwarder.setLevel(extensionLogLevel);
  logForwarder.setListener((level, ...args) => {
    // Write extension logs to the console.
    console.log(`qsharp: [${level}] ${args.join(" ")}`);
  });

  // Enable dev diagnostics in the Problems view
  const config = vscode.workspace.getConfiguration("Q#");
  await config.update(
    "dev.showDevDiagnostics",
    true,
    vscode.ConfigurationTarget.Global,
  );

  console.log(
    `qsharp-tests: activate() completed in ${performance.now() - start}ms`,
  );
}

/**
 * Waits until a condition is met, with a timeout.
 *
 * @param condition A function that returns true when the condition is met
 * @param wakeUpOn The event that we want to wake up to check the condition
 * @param timeoutMs If the condition is not met by this timeout, this function will throw
 * @param timeoutErrorMsg The custom error message to throw if the condition is not met
 */
export async function waitForCondition(
  condition: (...args: any[]) => boolean,
  wakeUpOn: vscode.Event<any>,
  timeoutMs: number,
  timeoutErrorMsg: string,
) {
  let disposable: vscode.Disposable | undefined;
  await new Promise<void>((resolve, reject) => {
    let done = false;
    setTimeout(() => {
      if (!done) {
        reject(new Error(timeoutErrorMsg));
      }
    }, timeoutMs);

    disposable = wakeUpOn(() => {
      if (!done && condition()) {
        done = true;
        resolve();
      }
    });

    // Resolve immediately if condition is already met
    if (condition()) {
      done = true;
      resolve();
    }
  });
  disposable?.dispose();
}

/**
 * Convenience method to add a time delay.
 *
 * @deprecated While this function is convenient for local development,
 * please do NOT use it in actual tests. Instead, use `waitForCondition`
 * with an appropriate condition and wake up event.
 *
 * Adding an unconditional delay to tests is guaranteed to slow tests down
 * unnecessarily. It also reduces reliability when we're not clear about
 * exactly what we're waiting on.
 */
export async function delay(timeoutMs: number) {
  try {
    await waitForCondition(
      () => false,
      () => ({ dispose() {} }),
      timeoutMs,
      "hit the expected timeout",
    );
  } catch {
    // expected
  }
}

/**
 * Waits for diagnostics to meet a specific condition using the onDidChangeDiagnostics event.
 *
 * @param uri The URI to get diagnostics for
 * @param condition A function that returns true when the diagnostics meet the expected condition
 * @param timeoutMs Timeout for waiting for the condition to be met
 * @param timeoutErrorMsg Custom error message if timeout is reached
 */
async function waitForDiagnostics(
  uri: vscode.Uri,
  condition: (diagnostics: vscode.Diagnostic[]) => boolean,
  timeoutMs: number = 2000,
  timeoutErrorMsg: string = "Diagnostics condition not met within timeout",
): Promise<vscode.Diagnostic[]> {
  await waitForCondition(
    () => {
      const diagnostics = vscode.languages.getDiagnostics(uri);
      return condition(diagnostics);
    },
    vscode.languages.onDidChangeDiagnostics,
    timeoutMs,
    timeoutErrorMsg,
  );

  // Return the diagnostics that met the condition
  return vscode.languages.getDiagnostics(uri);
}

/**
 * Waits for diagnostics to appear (non-empty) for a specific URI.
 * Ignores the document status diagnostic.
 *
 * @param uri The URI to wait for diagnostics on
 * @param timeoutMs Timeout for waiting
 */
export async function waitForDiagnosticsToAppear(
  uri: vscode.Uri,
  timeoutMs: number = 2000,
): Promise<vscode.Diagnostic[]> {
  const diagnostics = await waitForDiagnostics(
    uri,
    (diagnostics) =>
      // Filter out the dev diagnostics to return only real diagnostics
      diagnostics.filter((d) => d.code !== documentStatusDiagnosticCode)
        .length > 0,
    timeoutMs,
    `Expected diagnostics to appear for ${uri.path} within timeout`,
  );

  return diagnostics.filter((d) => d.code !== documentStatusDiagnosticCode);
}

/**
 * Waits for diagnostics to be empty for a specific URI.
 * Ignores the document status diagnostic.
 *
 * @param uri The URI to wait for empty diagnostics on
 * @param timeoutMs Timeout for waiting
 */
export async function waitForDiagnosticsToBeEmpty(
  uri: vscode.Uri,
  timeoutMs: number = 2000,
): Promise<void> {
  await waitForDiagnostics(
    uri,
    (diagnostics) =>
      // Filter out the dev diagnostics to return only real diagnostics
      diagnostics.filter((d) => d.code !== documentStatusDiagnosticCode)
        .length === 0,
    timeoutMs,
    `Expected diagnostics to be empty for ${uri.path} within timeout`,
  );
}

/**
 * Opens a single document and waits for it to be processed by the language service.
 * The language service signals that it processed this version of the document by
 * publishing an info diagnostic on the document with its known status.
 * (This diagnostic is not visible in the product, but only enabled via a
 * configuration setting in tests.)
 *
 * @param documentUri The URI of the document to open
 * @param timeoutMs Timeout for waiting for the document to be processed
 */
export async function openDocumentAndWaitForProcessing(
  documentUri: vscode.Uri,
  timeoutMs: number = 2000,
): Promise<vscode.TextDocument> {
  const doc = await vscode.workspace.openTextDocument(documentUri);
  const version = doc.version;

  // Wait for the status diagnostic to appear, indicating the document is loaded
  await waitForDiagnostics(
    documentUri,
    (diagnostics) =>
      diagnostics.some(
        (d) =>
          d.code === documentStatusDiagnosticCode &&
          d.message.includes(`version=${version}`),
      ),
    timeoutMs,
    `Document ${documentUri.path} was not processed within timeout ${timeoutMs}ms`,
  );

  return doc;
}
