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
  isRendererToExtensionMessage,
  MIME_TYPE,
  parseLearningPayload,
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

export const activate: ActivationFunction = (context: RendererContext) => {
  const postMessage = (message: RendererToExtensionMessage) => {
    if (context.postMessage === undefined) {
      // No extension host — an exported HTML page, say.
      return false;
    }

    // Built here, but from payload values, so this is where a quiz id that
    // would not survive the host's check is dropped. Sending the action
    // without it still opens chat; sending it whole would be ignored.
    const fallback: RendererToExtensionMessage | undefined =
      message.type === "qdk-learning/action"
        ? {
            type: "qdk-learning/action",
            rendererId: RENDERER_ID,
            actionId: message.actionId,
          }
        : undefined;
    const safe = isRendererToExtensionMessage(message) ? message : fallback;
    if (!safe) {
      console.warn("QDK Learning: discarding an invalid quiz answer message.");
      return false;
    }

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
              postMessage,
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

  return parseLearningPayload(outputItem.json());
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
