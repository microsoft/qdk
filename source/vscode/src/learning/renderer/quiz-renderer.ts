// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/**
 * Notebook output renderer for `application/vnd.qdk.quiz+json`.
 *
 * The payload is self-contained, so a baked notebook stays interactive with no
 * kernel running — the renderer only ever turns saved data into DOM.
 */

interface QuizPayload {
  schema: number;
  question: string;
  options: string[];
  answerIndex: number;
  explanation?: string;
}

interface OutputItem {
  json(): QuizPayload;
}

const BUTTON_STYLE =
  "display:block;width:100%;text-align:left;margin:4px 0;padding:6px 10px;" +
  "font:inherit;cursor:pointer;border-radius:4px;" +
  "border:1px solid var(--vscode-panel-border);" +
  "background:var(--vscode-button-secondaryBackground);" +
  "color:var(--vscode-button-secondaryForeground)";

export function activate() {
  return {
    renderOutputItem(item: OutputItem, element: HTMLElement) {
      // The same element is reused when an output re-renders.
      element.replaceChildren();

      const quiz = item.json();
      if (quiz?.schema !== 1) {
        const fallback = new Error(`Unsupported quiz schema: ${quiz?.schema}`);
        fallback.name = "vscode.fallbackToNextRenderer";
        throw fallback;
      }

      const root = document.createElement("div");
      root.style.cssText =
        "font-family:var(--vscode-font-family);color:var(--vscode-foreground);" +
        "margin:8px 0;max-width:48em";

      const question = document.createElement("div");
      question.textContent = quiz.question;
      question.style.cssText = "font-weight:600;margin-bottom:10px";
      root.append(question);

      const result = document.createElement("div");
      result.style.cssText = "margin-top:10px";

      const buttons = quiz.options.map((option, index) => {
        const button = document.createElement("button");
        button.textContent = option;
        button.style.cssText = BUTTON_STYLE;
        button.addEventListener("click", () => {
          const correct = index === quiz.answerIndex;
          const colour = correct
            ? "var(--vscode-testing-iconPassed)"
            : "var(--vscode-testing-iconFailed)";

          for (const other of buttons) {
            other.disabled = true;
            other.style.cursor = "default";
          }
          button.style.borderColor = colour;
          button.style.color = colour;

          result.replaceChildren();
          const verdict = document.createElement("div");
          verdict.textContent = correct ? "Correct." : "Not quite.";
          verdict.style.cssText = `font-weight:600;color:${colour}`;
          result.append(verdict);

          if (quiz.explanation) {
            const explanation = document.createElement("div");
            explanation.textContent = quiz.explanation;
            explanation.style.cssText = "margin-top:6px;opacity:0.9";
            result.append(explanation);
          }
        });
        root.append(button);
        return button;
      });

      root.append(result);
      element.append(root);
    },
  };
}
