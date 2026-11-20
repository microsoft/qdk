// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  MAJORANA_VIEWS,
  type MajoranaView,
  type MajoranaPresentationOptions,
} from "./types.js";

export type MajoranaControlsProps = {
  view: MajoranaView;
  step: number;
  maximumStep: number;
  playing: boolean;
  presentation: Required<MajoranaPresentationOptions>;
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
  step,
  maximumStep,
  playing,
  presentation: { showQubitLabels, showMzmLabels },
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
          <div class="qs-majorana-keyboard-help">
            <IconButton label="Keyboard shortcuts" icon="info" />
          </div>
          <IconButton label="Zoom in" icon="zoom-in" onClick={onZoomIn} />
          <IconButton label="Zoom out" icon="zoom-out" onClick={onZoomOut} />
        </div>
        <div
          class="qs-majorana-view-control"
          role="group"
          aria-label="Visualization view"
        >
          {MAJORANA_VIEWS.map((option) => (
            <button
              key={option}
              class={`qs-majorana-view-button ${
                view === option ? "qs-majorana-view-button-selected" : ""
              }`}
              type="button"
              aria-pressed={view === option}
              aria-label={option}
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
          {(
            [
              ["Qubit labels", showQubitLabels, onShowQubitLabelsChange],
              ["MZM labels", showMzmLabels, onShowMzmLabelsChange],
            ] as const
          ).map(([label, checked, onChange]) => (
            <label key={label}>
              <input
                type="checkbox"
                checked={checked}
                aria-label={label}
                onChange={(event) => onChange(event.currentTarget.checked)}
              />
              <span class="qs-majorana-option-label">{label}</span>
            </label>
          ))}
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
      <KeyboardHelpPanel />
    </div>
  );
}

function KeyboardHelpPanel() {
  return (
    <div class="qs-majorana-keyboard-help-panel" role="tooltip">
      <strong>Keyboard shortcuts</strong>
      <div class="qs-majorana-keyboard-help-grid">
        {[
          ["Left", "Step back"],
          ["Right", "Step forward"],
          ["Up", "Zoom in"],
          ["Down", "Zoom out"],
          ["P", "Play / pause"],
          ["F", "Faster playback"],
          ["S", "Slower playback"],
          ["T", "Toggle Tetrons / Qubits"],
        ].flatMap(([key, description]) => [
          <span key={key}>{key}</span>,
          <span key={`${key}-description`}>{description}</span>,
        ])}
      </div>
    </div>
  );
}

const ICON_PATHS = {
  info: "M12 10.875v6.75",
  "zoom-out": "m5 10 7 6 7-6",
  "zoom-in": "m5 14 7-6 7 6",
  previous: "M11 6 4 12l7 6V6zm9 0-7 6 7 6V6z",
  play: "M7 5v14l12-7L7 5z",
  pause: "M7 5h4v14H7V5zm6 0h4v14h-4V5z",
  next: "m4 6 7 6-7 6V6zm9 0 7 6-7 6V6z",
} as const;
type IconName = keyof typeof ICON_PATHS;

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
      <svg viewBox="0 0 24 24" aria-hidden="true">
        {icon === "info" && (
          <circle class="qs-majorana-info-dot" cx="12" cy="6.375" r="1.125" />
        )}
        <path
          class={icon === "info" ? "qs-majorana-info-line" : undefined}
          d={ICON_PATHS[icon]}
        />
      </svg>
    </button>
  );
}
