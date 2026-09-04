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
    if (
      context.postMessage === undefined ||
      !isRendererToExtensionMessage(message)
    ) {
      return false;
    }

    void context.postMessage(message);
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

  return payload as unknown as LearningPayload;
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
  root.dataset.rendererId = RENDERER_ID;

  const title = document.createElement("strong");
  title.textContent = "Unable to render QDK learning output.";
  const details = document.createElement("pre");
  details.textContent = error instanceof Error ? error.message : String(error);

  root.append(title, details);
  element.replaceChildren(root);
}
