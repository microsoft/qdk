// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * An error thrown by a tool when the tool cannot complete its task.
 * The message will be shown to the user.
 */
export class CopilotToolError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CopilotToolError";
  }
}
