// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

/**
 * Confirm-dialog primitive used by destructive editor flows
 * (currently only "remove a qubit line that has operations attached").
 *
 * Standalone so individual controllers can use it without depending
 * on the full `CircuitEvents` class.
 *
 * @param message - Text shown in the prompt body.
 * @param callback - Invoked with `true` on OK, `false` on Cancel.
 */
export const _createConfirmPrompt = (
  message: string,
  callback: (confirmed: boolean) => void,
) => {
  const overlay = document.createElement("div");
  overlay.classList.add("prompt-overlay");
  overlay.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  const confirmContainer = document.createElement("div");
  confirmContainer.classList.add("prompt-container");

  const messageElem = document.createElement("div");
  messageElem.classList.add("prompt-message");
  messageElem.textContent = message;

  const buttonsContainer = document.createElement("div");
  buttonsContainer.classList.add("prompt-buttons");

  const okButton = document.createElement("button");
  okButton.classList.add("prompt-button");
  okButton.textContent = "OK";
  okButton.addEventListener("click", () => {
    callback(true);
    document.body.removeChild(overlay);
    document.removeEventListener("keydown", handleGlobalKeyDown, true);
  });

  const cancelButton = document.createElement("button");
  cancelButton.classList.add("prompt-button");
  cancelButton.textContent = "Cancel";
  cancelButton.addEventListener("click", () => {
    callback(false);
    document.body.removeChild(overlay);
    document.removeEventListener("keydown", handleGlobalKeyDown, true);
  });

  // Handle Enter (commit) and Escape (cancel) globally while the
  // prompt is open. Capture-phase so we don't fight any descendant
  // handlers in the editor surface.
  const handleGlobalKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Enter") {
      event.preventDefault();
      okButton.click();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelButton.click();
    }
  };
  document.addEventListener("keydown", handleGlobalKeyDown, true);

  buttonsContainer.appendChild(okButton);
  buttonsContainer.appendChild(cancelButton);

  confirmContainer.appendChild(messageElem);
  confirmContainer.appendChild(buttonsContainer);

  overlay.appendChild(confirmContainer);
  document.body.appendChild(overlay);

  // Drop focus from whatever was focused so Enter/Escape go through
  // our document-level handler instead of any input that had it.
  if (document.activeElement) {
    (document.activeElement as HTMLElement).blur();
  }
};
