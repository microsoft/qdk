// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { MajoranaView } from "./types.js";

export type MajoranaControlsProps = {
  view: MajoranaView;
  availableViews: readonly MajoranaView[];
  step: number;
  maximumStep: number;
  playing: boolean;
  showQubitLabels: boolean;
  showMzmLabels: boolean;
  onViewChange(view: MajoranaView): void;
  onStepChange(step: number): void;
  onPlayPause(): void;
  onZoomIn(): void;
  onZoomOut(): void;
  onShowQubitLabelsChange(show: boolean): void;
  onShowMzmLabelsChange(show: boolean): void;
};

export function MajoranaControls({
  view,
  availableViews,
  step,
  maximumStep,
  playing,
  showQubitLabels,
  showMzmLabels,
  onViewChange,
  onStepChange,
  onPlayPause,
  onZoomIn,
  onZoomOut,
  onShowQubitLabelsChange,
  onShowMzmLabelsChange,
}: MajoranaControlsProps) {
  return (
    <div class="qs-majorana-controls">
      <div class="qs-majorana-toolstrip">
        <div
          class="qs-majorana-zoom-controls"
          role="group"
          aria-label="Visualization zoom"
        >
          <KeyboardHelp />
          <IconButton label="Zoom in" icon="zoom-in" onClick={onZoomIn} />
          <IconButton label="Zoom out" icon="zoom-out" onClick={onZoomOut} />
        </div>
        <div
          class="qs-majorana-view-control"
          role="group"
          aria-label="Visualization view"
        >
          {availableViews.map((option) => (
            <button
              key={option}
              class={`qs-majorana-view-button ${
                view === option ? "qs-majorana-view-button-selected" : ""
              }`}
              type="button"
              aria-pressed={view === option}
              aria-label={option === "Virtual" ? "Virtual Qubits" : option}
              onClick={() => onViewChange(option)}
            >
              <span class="qs-majorana-view-label-full">{option}</span>
              <span class="qs-majorana-view-label-compact" aria-hidden="true">
                {option[0]}
              </span>
            </button>
          ))}
        </div>
        <div class="qs-majorana-label-controls">
          <label>
            <input
              type="checkbox"
              checked={showQubitLabels}
              aria-label="Qubit labels"
              onChange={(event) =>
                onShowQubitLabelsChange(event.currentTarget.checked)
              }
            />
            <span class="qs-majorana-option-label">Qubit labels</span>
          </label>
          {view !== "Virtual" && (
            <label>
              <input
                type="checkbox"
                checked={showMzmLabels}
                aria-label="MZM labels"
                onChange={(event) =>
                  onShowMzmLabelsChange(event.currentTarget.checked)
                }
              />
              <span class="qs-majorana-option-label">MZM labels</span>
            </label>
          )}
        </div>
        <label class="qs-majorana-scrubber-label">
          <span class="qs-majorana-step">
            {step} / {maximumStep}
          </span>
          <input
            class="qs-majorana-scrubber"
            type="range"
            min="0"
            max={maximumStep}
            step="1"
            value={step}
            aria-label="Visualization step"
            onInput={(event) => onStepChange(Number(event.currentTarget.value))}
          />
        </label>
        <div
          class="qs-majorana-playback-controls"
          role="group"
          aria-label="Playback"
        >
          <IconButton
            label="Previous visualization step"
            icon="previous"
            disabled={step === 0}
            onClick={() => onStepChange(step - 1)}
          />
          <IconButton
            label={playing ? "Pause playback" : "Play visualization"}
            icon={playing ? "pause" : "play"}
            pressed={playing}
            onClick={onPlayPause}
          />
          <IconButton
            label="Next visualization step"
            icon="next"
            disabled={step === maximumStep}
            onClick={() => onStepChange(step + 1)}
          />
        </div>
      </div>
      <KeyboardHelpPanel availableViews={availableViews} />
    </div>
  );
}

function KeyboardHelp() {
  return (
    <div class="qs-majorana-keyboard-help">
      <IconButton label="Keyboard shortcuts" icon="info" />
    </div>
  );
}

function KeyboardHelpPanel({
  availableViews,
}: {
  availableViews: readonly MajoranaView[];
}) {
  return (
    <div class="qs-majorana-keyboard-help-panel" role="tooltip">
      <strong>Keyboard shortcuts</strong>
      <div class="qs-majorana-keyboard-help-grid">
        <span>Left</span>
        <span>Step back</span>
        <span>Right</span>
        <span>Step forward</span>
        <span>Up</span>
        <span>Zoom in</span>
        <span>Down</span>
        <span>Zoom out</span>
        <span>P</span>
        <span>Play / pause</span>
        <span>F</span>
        <span>Faster playback</span>
        <span>S</span>
        <span>Slower playback</span>
        <span>T</span>
        <span>
          {availableViews.includes("Virtual")
            ? "Cycle Tetrons / Qubits / Virtual"
            : "Toggle Tetrons / Qubits"}
        </span>
      </div>
    </div>
  );
}

type IconName =
  | "info"
  | "zoom-out"
  | "zoom-in"
  | "previous"
  | "play"
  | "pause"
  | "next";

function IconButton({
  label,
  icon,
  disabled,
  pressed,
  onClick,
}: {
  label: string;
  icon: IconName;
  disabled?: boolean;
  pressed?: boolean;
  onClick?(): void;
}) {
  return (
    <button
      class="qs-majorana-icon-button"
      type="button"
      aria-label={label}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
    >
      <ControlIcon name={icon} />
    </button>
  );
}

function ControlIcon({ name }: { name: IconName }) {
  switch (name) {
    case "info":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <circle class="qs-majorana-info-dot" cx="12" cy="6.375" r="1.125" />
          <path class="qs-majorana-info-line" d="M12 10.875v6.75" />
        </svg>
      );
    case "zoom-out":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="m5 10 7 6 7-6" />
        </svg>
      );
    case "zoom-in":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="m5 14 7-6 7 6" />
        </svg>
      );
    case "previous":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M11 6 4 12l7 6V6zm9 0-7 6 7 6V6z" />
        </svg>
      );
    case "play":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M7 5v14l12-7L7 5z" />
        </svg>
      );
    case "pause":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M7 5h4v14H7V5zm6 0h4v14h-4V5z" />
        </svg>
      );
    case "next":
      return (
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="m4 6 7 6-7 6V6zm9 0 7 6-7 6V6z" />
        </svg>
      );
  }
}
