// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type {
  ActivationFunction,
  OutputItem,
  RendererContext,
} from "vscode-notebook-renderer";
import { renderMultipleChoice } from "./multipleChoice.js";
import type { LearningPayload, RendererToExtensionMessage } from "./schema.js";
import {
  isRecord,
  isRendererToExtensionMessage,
  MIME_TYPE,
  RENDERER_ID,
} from "./schema.js";
// Bundled as text by the renderer build so the styles can be injected here —
// VS Code loads the renderer as a lone JS module and won't fetch a sibling
// stylesheet. `qdk-theme.css` supplies the shared `--qdk-*` palette and must
// come first so our rules can build on it.
import themeCss from "../../../npm/qsharp/ux/qdk-theme.css";
import rendererCss from "./styles.css";

const STYLE_ELEMENT_ID = "qdk-learning-renderer-styles";

/** Add the stylesheet to the output webview once per document. */
function ensureStyles() {
  if (document.getElementById(STYLE_ELEMENT_ID)) {
    return;
  }

  const style = document.createElement("style");
  style.id = STYLE_ELEMENT_ID;
  style.textContent = `${themeCss}\n${rendererCss}`;
  document.head.appendChild(style);
}

type Cleanup = () => void;

const cleanupByOutputId = new Map<string, Cleanup>();
const cleanupByElement = new WeakMap<HTMLElement, Cleanup>();

export const activate: ActivationFunction<void> = (
  context: RendererContext<void>,
) => {
  const postAction = (message: RendererToExtensionMessage) => {
    if (context.postMessage === undefined) {
      // No extension host — an exported HTML page, say.
      return false;
    }

    // Built here, but from payload values, so this is where a quiz id that
    // would not survive the host's check is dropped. Sending the action
    // without it still opens chat; sending it whole would be ignored.
    const { actionId } = message;
    const safe: RendererToExtensionMessage = isRendererToExtensionMessage(
      message,
    )
      ? message
      : { type: "qdk-learning/action", rendererId: RENDERER_ID, actionId };

    void context.postMessage(safe);
    return true;
  };

  return {
    renderOutputItem(outputItem: OutputItem, element: HTMLElement) {
      ensureStyles();
      cleanupOutput(outputItem.id);
      cleanupElement(element);
      element.replaceChildren();

      const disposables: Cleanup[] = [];
      const cleanup = () => {
        for (const dispose of disposables.splice(0)) {
          dispose();
        }
        if (cleanupByElement.get(element) === cleanup) {
          element.replaceChildren();
          cleanupByElement.delete(element);
        }
        cleanupByOutputId.delete(outputItem.id);
      };

      cleanupByOutputId.set(outputItem.id, cleanup);
      cleanupByElement.set(element, cleanup);

      try {
        const payload = readPayload(outputItem);
        switch (payload.kind) {
          case "multiple-choice":
            renderMultipleChoice(payload, element, {
              postAction,
              addDisposable: (dispose) => disposables.push(dispose),
            });
            break;
        }
      } catch (error) {
        cleanup();
        renderError(element, error);
      }
    },
    disposeOutputItem(id?: string) {
      // VS Code calls this with no id for "Clear All Outputs", so treating
      // the parameter as required would leak every listener in the document.
      if (id === undefined) {
        for (const cleanup of [...cleanupByOutputId.values()]) {
          cleanup();
        }
        return;
      }
      cleanupOutput(id);
    },
  };
};

function readPayload(outputItem: OutputItem): LearningPayload {
  if (outputItem.mime !== MIME_TYPE) {
    throw new Error(`Unsupported MIME type: ${outputItem.mime}`);
  }

  const value: unknown = outputItem.json();
  if (!isRecord(value)) {
    throw new Error("Expected a QDK learning payload object.");
  }

  const payload = value;

  // Say which side is ahead. A notebook can outlive the extension that wrote
  // it, and "update the QDK extension" is a far more useful thing to read in a
  // cell than a generic parse failure.
  if (payload.schemaVersion !== SUPPORTED_SCHEMA_VERSION) {
    throw new Error(
      `This output uses QDK learning payload version ${String(payload.schemaVersion)}, ` +
        `but this renderer supports version ${SUPPORTED_SCHEMA_VERSION}. ` +
        "Update the QDK extension to view it.",
    );
  }

  if (!isSupportedKind(payload.kind)) {
    throw new Error(
      `Unknown QDK learning output kind "${String(payload.kind)}". ` +
        `This renderer knows: ${SUPPORTED_KINDS.join(", ")}.`,
    );
  }

  if (payload.cellId !== undefined && typeof payload.cellId !== "string") {
    throw new Error("QDK learning payload has a non-string cellId.");
  }

  // Per-kind checks stay behind the kind test: a second payload kind must not
  // have to satisfy the multiple-choice shape.
  switch (payload.kind) {
    case "multiple-choice":
      assertMultipleChoice(payload);
      break;
  }

  return payload as unknown as LearningPayload;
}

/**
 * Check the fields the question is actually drawn and graded from.
 *
 * Version and kind only describe the envelope. Without this a payload that
 * survived a hand edit could set `correct: "false"`, which is a non-empty
 * string and therefore truthy, and a wrong option would be marked right — so
 * these are checked before anything is rendered rather than trusted.
 */
function assertMultipleChoice(payload: Record<string, unknown>): void {
  if (typeof payload.prompt !== "string" || payload.prompt.length === 0) {
    throw new Error("QDK learning payload has no question text.");
  }

  if (
    payload.multiSelect !== undefined &&
    typeof payload.multiSelect !== "boolean"
  ) {
    throw new Error("QDK learning payload has a non-boolean multiSelect.");
  }

  if (!Array.isArray(payload.options) || payload.options.length === 0) {
    throw new Error("QDK learning payload has no options.");
  }

  const ids = new Set<string>();
  for (const option of payload.options) {
    if (!isRecord(option)) {
      throw new Error("QDK learning payload has a malformed option.");
    }
    if (typeof option.id !== "string" || option.id.length === 0) {
      throw new Error("QDK learning payload has an option with no id.");
    }
    if (ids.has(option.id)) {
      throw new Error(
        `QDK learning payload reuses the option id "${option.id}".`,
      );
    }
    ids.add(option.id);

    if (typeof option.text !== "string" || option.text.length === 0) {
      throw new Error(
        `QDK learning option "${option.id}" has no text to show.`,
      );
    }
    if (typeof option.correct !== "boolean") {
      throw new Error(
        `QDK learning option "${option.id}" does not say whether it is correct.`,
      );
    }
    if (
      option.explanation !== undefined &&
      typeof option.explanation !== "string"
    ) {
      throw new Error(
        `QDK learning option "${option.id}" has a non-string explanation.`,
      );
    }
  }

  // Cardinality mirrors `_normalize_options` in `_learning_output.py`: the two
  // sides describe the same payload, and a notebook can outlive — or bypass —
  // the emitter that wrote it.
  const correct = payload.options.filter(
    (option) => (option as { correct: boolean }).correct,
  ).length;

  if (correct === 0) {
    throw new Error("QDK learning payload has no correct option.");
  }

  if (payload.multiSelect === true) {
    if (correct < 2) {
      throw new Error(
        "QDK learning payload says select all that apply but marks one option correct.",
      );
    }
    if (correct === payload.options.length) {
      throw new Error(
        "QDK learning payload marks every option correct, so it cannot be answered wrongly.",
      );
    }
  } else if (correct > 1) {
    // Grading compares the selected set with the correct set, and a radio
    // group holds one selection, so this question could never be answered
    // right. Refusing to draw it beats showing an unwinnable one.
    throw new Error(
      `QDK learning payload marks ${correct} options correct but is not multi-select.`,
    );
  }
}

/** The only payload version this renderer understands. */
const SUPPORTED_SCHEMA_VERSION = 1;

const SUPPORTED_KINDS = ["multiple-choice"] as const;

function isSupportedKind(
  kind: unknown,
): kind is (typeof SUPPORTED_KINDS)[number] {
  return (
    typeof kind === "string" &&
    SUPPORTED_KINDS.includes(kind as (typeof SUPPORTED_KINDS)[number])
  );
}

function cleanupOutput(id: string) {
  cleanupByOutputId.get(id)?.();
}

function cleanupElement(element: HTMLElement) {
  cleanupByElement.get(element)?.();
}

function renderError(element: HTMLElement, error: unknown) {
  const root = document.createElement("section");
  root.className = "qdk-learning qdk-learning-error";

  const title = document.createElement("strong");
  title.textContent = "Unable to render QDK learning output.";
  const details = document.createElement("pre");
  details.textContent = error instanceof Error ? error.message : String(error);

  root.append(title, details);
  element.replaceChildren(root);
}
