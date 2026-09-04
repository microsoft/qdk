// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { MultipleChoicePayload } from "./schema.js";
import { RENDERER_ID } from "./schema.js";
import {
  appendRichTextElement,
  appendTextElement,
  createActionButton,
  setLiveRegion,
  type RenderContext,
} from "./rendering.js";

let groupId = 0;

export function renderMultipleChoice(
  payload: MultipleChoicePayload,
  element: HTMLElement,
  context: RenderContext,
): void {
  const root = document.createElement("section");
  root.className = "qdk-learning qdk-learning-card";

  // The band the chemistry course already uses to mark a self-check question,
  // so an interactive question reads as the same thing the collapsible ones
  // were. Decorative: the legend below carries the question for a reader.
  const header = document.createElement("div");
  header.className = "qdk-learning-quiz-header";
  const mark = appendTextElement(
    header,
    "span",
    "qdk-learning-quiz-mark",
    "\u2753",
  );
  mark.setAttribute("aria-hidden", "true");
  appendTextElement(
    header,
    "span",
    "qdk-learning-quiz-label",
    "Check your understanding",
  );
  root.appendChild(header);

  // Fieldset/legend keeps the answer controls grouped for screen readers.
  const fieldset = document.createElement("fieldset");
  fieldset.className = "qdk-learning-options";
  const legend = document.createElement("legend");
  legend.className = "qdk-learning-prompt";
  appendRichTextElement(legend, "span", "", payload.prompt);

  // Said out loud, and inside the legend so a screen reader hears it with the
  // question rather than after it. A learner who assumes one answer would stop
  // at the first correct option and be marked wrong for a question they
  // actually understood.
  if (payload.multiSelect) {
    appendTextElement(
      legend,
      "span",
      "qdk-learning-prompt-hint",
      "Select all that apply.",
    );
  }
  fieldset.appendChild(legend);

  const feedback = appendTextElement(root, "p", "qdk-learning-feedback", "");
  feedback.hidden = true;
  setLiveRegion(feedback);

  const controls = document.createElement("div");
  controls.className = "qdk-learning-controls";

  const actionList = document.createElement("div");
  actionList.className = "qdk-learning-action-list";
  actionList.hidden = true;

  // Include the cell id when available, plus a counter to avoid cross-output
  // radio grouping even if a notebook renders duplicate cell ids.
  const groupName = `qdk-learning-${payload.cellId ?? "output"}-${groupId++}`;
  const optionViews: OptionView[] = [];
  for (const [index, option] of payload.options.entries()) {
    const letter = optionLetter(index);
    const label = document.createElement("label");
    label.className = "qdk-learning-option";

    const input = document.createElement("input");
    input.type = payload.multiSelect ? "checkbox" : "radio";
    input.name = groupName;
    input.value = option.id;

    // Selecting never grades. Arrow keys move between radios and fire
    // `change`, so grading here would submit whichever option a keyboard
    // user landed on first and disable the rest of the group.
    const onChange = () => {
      syncSelectedStates();
      updateCheckButton();
    };
    input.addEventListener("change", onChange);
    context.addDisposable(() => input.removeEventListener("change", onChange));

    const badge = document.createElement("span");
    badge.className = "qdk-learning-option-badge";
    badge.setAttribute("aria-hidden", "true");
    badge.textContent = letter;

    const verdict = document.createElement("span");
    verdict.className = "qdk-learning-verdict qdk-learning-sr-only";

    const body = document.createElement("span");
    body.className = "qdk-learning-option-body";

    appendRichTextElement(
      body,
      "span",
      "qdk-learning-option-text",
      option.text,
    );

    label.append(input, badge, body, verdict);
    fieldset.appendChild(label);
    optionViews.push({ option, input, label, badge, verdict, body, letter });
  }

  const checkButton = document.createElement("button");
  checkButton.type = "button";
  checkButton.textContent = "Check answer";
  checkButton.disabled = true;
  const onCheck = () => evaluateAnswer();
  checkButton.addEventListener("click", onCheck);
  context.addDisposable(() =>
    checkButton.removeEventListener("click", onCheck),
  );

  const tryAgainButton = document.createElement("button");
  tryAgainButton.type = "button";
  tryAgainButton.textContent = "Try again";
  const onTryAgain = () => resetAnswer();
  tryAgainButton.addEventListener("click", onTryAgain);
  context.addDisposable(() =>
    tryAgainButton.removeEventListener("click", onTryAgain),
  );

  // Built once, not per grading: a learner can cycle Check/Try again any number
  // of times, and creating a fresh button each time would retain a detached
  // node and its listener for the life of the output.
  let lastSelectedIds = new Set<string>();
  const whyWrongButton = createActionButton("Why is that wrong?");
  const onWhyWrong = () => {
    const posted = context.postAction({
      type: "qdk-learning/action",
      rendererId: RENDERER_ID,
      actionId: "why-wrong",
      cellId: payload.cellId,
      context: {
        question: payload.prompt,
        choice: optionViews
          .filter((view) => lastSelectedIds.has(view.option.id))
          .map((view) => view.option.text)
          .join("; "),
      },
    });

    if (!posted) {
      // Exported HTML has no extension channel, so replace the inert action.
      appendActionUnavailableNote(actionList);
    }
  };
  whyWrongButton.addEventListener("click", onWhyWrong);
  context.addDisposable(() =>
    whyWrongButton.removeEventListener("click", onWhyWrong),
  );

  controls.appendChild(checkButton);

  root.append(fieldset, feedback, controls, actionList);
  element.appendChild(root);

  function evaluateAnswer(): void {
    const selectedIds = new Set(
      optionViews
        .filter((view) => view.input.checked)
        .map((view) => view.option.id),
    );
    const isCorrect = optionViews.every(
      (view) => selectedIds.has(view.option.id) === view.option.correct,
    );

    for (const view of optionViews) {
      const selected = selectedIds.has(view.option.id);
      view.label.dataset.selected = selected ? "true" : "false";
      if (selected && view.option.correct) {
        setOptionState(view, "correct");
      } else if (selected) {
        setOptionState(view, "incorrect");
      } else if (view.option.correct) {
        setOptionState(view, "missed");
      }

      if (view.option.explanation !== undefined) {
        appendRichTextElement(
          view.body,
          "span",
          "qdk-learning-option-why",
          view.option.explanation,
        );
      }
      view.input.disabled = true;
    }

    feedback.hidden = false;
    feedback.dataset.state = isCorrect ? "correct" : "incorrect";
    feedback.textContent = isCorrect
      ? "Correct!"
      : incorrectMessage(selectedIds);

    controls.replaceChildren(tryAgainButton);
    actionList.replaceChildren();
    actionList.hidden = true;

    if (!isCorrect) {
      showWhyWrongAction(selectedIds);
    }

    // Grading replaced the focused Check button, which would drop focus to the
    // document. Move it to the control that took its place.
    tryAgainButton.focus();
  }

  function resetAnswer(): void {
    for (const view of optionViews) {
      view.input.checked = false;
      view.input.disabled = false;
      delete view.label.dataset.state;
      delete view.label.dataset.selected;
      view.badge.textContent = view.letter;
      view.verdict.textContent = "";
      for (const why of Array.from(
        view.body.querySelectorAll(".qdk-learning-option-why"),
      )) {
        why.remove();
      }
    }
    feedback.hidden = true;
    feedback.textContent = "";
    delete feedback.dataset.state;
    actionList.replaceChildren();
    actionList.hidden = true;
    checkButton.disabled = true;
    controls.replaceChildren(checkButton);

    // Retrying detaches the Try again button, which is the element the learner
    // just activated, so focus would fall to the document. Send it to the first
    // answer instead: `Check answer` was disabled a line ago and a disabled
    // control cannot take focus, and the first option is where a retry starts
    // anyway. `:focus-within` paints the row, so the position stays visible.
    optionViews[0]?.input.focus();
  }

  function updateCheckButton(): void {
    checkButton.disabled = !optionViews.some((view) => view.input.checked);
  }

  /**
   * Say *how* the answer was wrong.
   *
   * On a multi-select, "having the right idea but missing one" and "picking a
   * wrong one" are different mistakes and deserve different nudges. On a
   * single-select there is only ever one way to be wrong, so the generic line
   * is the honest one.
   */
  function incorrectMessage(selectedIds: Set<string>): string {
    if (!payload.multiSelect) {
      return "Not quite. Review the marked choices, then try again.";
    }

    const missed = optionViews.filter(
      (view) => view.option.correct && !selectedIds.has(view.option.id),
    ).length;
    const wrong = optionViews.filter(
      (view) => !view.option.correct && selectedIds.has(view.option.id),
    ).length;

    if (missed > 0 && wrong === 0) {
      return missed === 1
        ? "Close — everything you picked is right, but one more applies."
        : `Close — everything you picked is right, but ${missed} more apply.`;
    }
    if (wrong > 0 && missed === 0) {
      return wrong === 1
        ? "Not quite — you found them all, but one choice doesn't apply."
        : `Not quite — you found them all, but ${wrong} choices don't apply.`;
    }
    return "Not quite. Review the marked choices, then try again.";
  }

  function syncSelectedStates(): void {
    for (const view of optionViews) {
      view.label.dataset.selected = view.input.checked ? "true" : "false";
    }
  }

  function showWhyWrongAction(selectedIds: Set<string>): void {
    lastSelectedIds = selectedIds;
    actionList.hidden = false;
    actionList.replaceChildren(whyWrongButton);
  }
}

type OptionView = {
  option: MultipleChoicePayload["options"][number];
  input: HTMLInputElement;
  label: HTMLLabelElement;
  badge: HTMLSpanElement;
  verdict: HTMLSpanElement;
  body: HTMLSpanElement;
  letter: string;
};

type OptionState = "correct" | "incorrect" | "missed";

function setOptionState(view: OptionView, state: OptionState): void {
  view.label.dataset.state = state;
  view.badge.textContent =
    state === "correct"
      ? "\u2713"
      : state === "incorrect"
        ? "\u2717"
        : "\u2022";
  view.verdict.textContent =
    state === "correct"
      ? "Correct choice"
      : state === "incorrect"
        ? "Incorrect choice"
        : "Missed correct choice";
}

/**
 * A, B, C… for the option badges.
 *
 * Wraps back to A past 26 rather than carrying into AA: a question with that
 * many options is an authoring problem, not a labelling one.
 */
function optionLetter(index: number): string {
  return String.fromCharCode(65 + (index % 26));
}

function appendActionUnavailableNote(parent: HTMLElement): void {
  parent.replaceChildren();
  appendTextElement(
    parent,
    "span",
    "qdk-learning-action-note",
    "This Copilot action is only available in VS Code.",
  );
}
