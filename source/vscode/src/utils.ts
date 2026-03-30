// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { qsharpLibraryUriScheme } from "qsharp-lang";
import * as vscode from "vscode";

// Guid format such as "00000000-1111-2222-3333-444444444444"
export function getRandomGuid(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));

  // Per https://www.ietf.org/rfc/rfc4122.txt, for UUID v4 (random GUIDs):
  // - Octet 6 contains the version in top 4 bits (0b0100)
  // - Octet 8 contains the variant in the top 2 bits (0b10)
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;

  // Convert the 16 bytes into 32 hex digits
  const hex = bytes.reduce(
    (acc, byte) => acc + byte.toString(16).padStart(2, "0"),
    "",
  );

  return (
    hex.substring(0, 8) +
    "-" +
    hex.substring(8, 12) +
    "-" +
    hex.substring(12, 16) +
    "-" +
    hex.substring(16, 20) +
    "-" +
    hex.substring(20, 32)
  );
}

/**
 * This is temporary until we're able to report proper stdlib and project URIs from
 * the wasm layer. See https://github.com/microsoft/qdk/blob/f8d344b32a1f1f918f3c91edf58c975db10f4370/wasm/src/diagnostic.rs
 *
 * @param maybeUri A source name returned from a Q# diagnostic
 * @returns A VS code URI that's okay to use in a Diagnostic object
 */
export function getSourceUri(maybeUri: string): vscode.Uri {
  // An error without a span (e.g. "no entrypoint found") gets reported as a "project-level" error.
  // See: https://github.com/microsoft/qdk/blob/f8d344b32a1f1f918f3c91edf58c975db10f4370/wasm/src/diagnostic.rs#L191
  // Ideally this would be a proper URI pointing to the project root or root document.
  // For now, make up a fake file path for display purposes.
  if (maybeUri === "<project>") {
    return vscode.Uri.file("Q# project");
  }

  try {
    return vscode.Uri.parse(maybeUri, true);
  } catch {
    // Not a URI, assume it's a filename from the stdlib
    // This URI should ideally be properly propagated from
    // https://github.com/microsoft/qdk/blob/f8d344b32a1f1f918f3c91edf58c975db10f4370/wasm/src/diagnostic.rs#L105
    return vscode.Uri.from({ scheme: qsharpLibraryUriScheme, path: maybeUri });
  }
}
