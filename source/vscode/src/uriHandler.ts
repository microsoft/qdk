// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { log } from "qsharp-lang";
import * as vscode from "vscode";

/**
 * A handler for a specific URI path routed through the extension's URI handler.
 *
 * Receives the parsed query parameters from the incoming URI. The path itself
 * is used for routing and does not need to be re-examined by the handler.
 */
export type UriRouteHandler = (params: URLSearchParams) => Promise<void>;

/**
 * A map from URI path (e.g. "/connectWorkspace") to its handler function.
 */
export type UriRoutes = Map<string, UriRouteHandler>;

/**
 * Registers the extension's URI handler with VS Code, dispatching incoming
 * URIs to the appropriate route handler based on the URI path.
 *
 * Requires `"onUri"` in the extension's `activationEvents` in package.json.
 *
 * @example
 * // In extension.ts activate():
 * const routes: UriRoutes = new Map([
 *   ["/connectWorkspace", handleConnectWorkspace],
 * ]);
 * context.subscriptions.push(registerUriHandler(routes));
 */
export function registerUriHandler(routes: UriRoutes): vscode.Disposable {
  return vscode.window.registerUriHandler({
    async handleUri(uri: vscode.Uri) {
      const handler = routes.get(uri.path);
      if (handler) {
        const params = new URLSearchParams(uri.query);
        await handler(params);
      } else {
        log.warn(`No URI route registered for path: ${uri.path}`);
      }
    },
  });
}
