// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { KatasServer } from "../server/index.js";
import {
  renderWelcome,
  renderNavigationItem,
  renderRunResult,
  renderSolutionCheck,
  renderCircuit,
  renderEstimate,
  renderProgress,
  renderKataList,
  renderHint,
  renderAnswer,
  renderCode,
} from "./render.js";
import {
  promptAction,
  promptKataJump,
  promptQuestion,
  promptShots,
} from "./prompts.js";

/** Show a status message while awaiting an async operation, then clear the line. */
async function withSpinner<T>(
  message: string,
  fn: () => Promise<T>,
): Promise<T> {
  process.stdout.write(message);
  try {
    const result = await fn();
    process.stdout.write("\r\x1b[K");
    return result;
  } catch (err) {
    process.stdout.write("\r\x1b[K");
    throw err;
  }
}

export async function runApp(
  server: KatasServer,
  hasAI: boolean,
): Promise<void> {
  console.log(renderWelcome());

  // Check if there's a saved position to resume
  try {
    const progress = server.getProgress();
    if (
      progress.stats.completedSections > 0 &&
      progress.currentPosition.kataId
    ) {
      console.log(
        `  Resuming: ${progress.stats.completedSections} sections completed.`,
      );
    }
  } catch {
    // No saved progress — that's fine
  }

  // Main navigation loop
  let running = true;
  let needsRender = true;
  while (running) {
    let pos;
    try {
      pos = server.getPosition();
    } catch {
      console.log("\nNo more content. You've reached the end!");
      console.log(renderProgress(server.getProgress()));
      break;
    }

    if (needsRender) {
      console.log(`\n${"─".repeat(60)}`);
      console.log(
        `  Kata: ${pos.kataId} | Section: ${pos.sectionId} | Item: ${pos.itemIndex + 1}`,
      );
      console.log("─".repeat(60));
      console.log("");
      console.log(renderNavigationItem(pos.item));
      console.log("");
    }
    needsRender = false;

    const actions = server.getAvailableActions();
    const action = await promptAction(actions);

    switch (action) {
      case "next": {
        const { moved } = server.next();
        if (!moved) {
          console.log("\n🎉 You've completed all content!");
          console.log(renderProgress(server.getProgress()));
          running = false;
        }
        needsRender = true;
        break;
      }

      case "back": {
        const { moved } = server.previous();
        if (!moved) {
          console.log("  Already at the beginning.");
        }
        needsRender = true;
        break;
      }

      case "run": {
        try {
          const { result } = await withSpinner("Running code...", () =>
            server.run(),
          );
          console.log(renderRunResult(result));
        } catch (err: unknown) {
          console.log(
            `Execution failed: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
        break;
      }

      case "run-noise": {
        const shots = await promptShots();
        try {
          const { result } = await withSpinner(
            `Running with noise (${shots} shots)...`,
            () => server.runWithNoise(shots),
          );
          console.log(renderRunResult(result));
        } catch (err: unknown) {
          console.log(
            `Execution failed: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
        break;
      }

      case "circuit": {
        try {
          const { result } = await withSpinner("Generating circuit...", () =>
            server.getCircuit(),
          );
          console.log(renderCircuit(result));
        } catch (err: unknown) {
          console.log(
            `Circuit generation failed: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
        break;
      }

      case "estimate": {
        try {
          const { result } = await withSpinner("Estimating resources...", () =>
            server.getResourceEstimate(),
          );
          console.log(renderEstimate(result));
        } catch (err: unknown) {
          console.log(
            `Resource estimation failed: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
        break;
      }

      case "check": {
        try {
          const { result } = await withSpinner("Checking solution...", () =>
            server.checkSolution(),
          );
          console.log(renderSolutionCheck(result));

          if (result.passed && hasAI) {
            console.log("\nWould you like an AI review of your solution?");
            const { result: review } = await server.reviewSolution();
            if (review) {
              console.log("\n🤖 AI Review:");
              console.log(review);
            }
          }
        } catch (err: unknown) {
          console.log(
            `Check failed: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
        break;
      }

      case "hint": {
        const { result: hint } = server.getNextHint();
        if (hint) {
          console.log("");
          console.log(renderHint(hint));
        } else {
          console.log("  No hints available.");
        }
        break;
      }

      case "ai-hint": {
        try {
          const { result: aiHint } = await withSpinner(
            "Getting AI hint...",
            () => server.getAIHint(),
          );
          if (aiHint) {
            console.log("🤖 AI Hint:");
            console.log(aiHint);
          } else {
            console.log("  AI hints not available.");
          }
        } catch (err: unknown) {
          console.log(
            `AI hint failed: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
        break;
      }

      case "solution": {
        const solution = server.getFullSolution();
        console.log("");
        console.log("📖 Reference Solution");
        console.log("");
        console.log(renderCode(solution));
        break;
      }

      case "reveal-answer": {
        const { result: answer } = server.revealAnswer();
        console.log("");
        console.log(renderAnswer(answer));
        break;
      }

      case "ask-ai": {
        const question = await promptQuestion();
        if (question.trim()) {
          try {
            const { result: answer } = await withSpinner("Asking AI...", () =>
              server.askConceptQuestion(question),
            );
            if (answer) {
              console.log("🤖 " + answer);
            } else {
              console.log("  AI not available.");
            }
          } catch (err: unknown) {
            console.log(
              `AI query failed: ${err instanceof Error ? err.message : String(err)}`,
            );
          }
        }
        break;
      }

      case "progress": {
        console.log("");
        console.log(renderProgress(server.getProgress()));
        break;
      }

      case "menu": {
        const katas = server.listKatas();
        console.log("");
        console.log(renderKataList(katas));
        console.log("");
        const jump = await promptKataJump(katas);
        if (jump) {
          try {
            server.goTo(jump.kataId);
          } catch (err: unknown) {
            console.log(err instanceof Error ? err.message : String(err));
          }
        }
        needsRender = true;
        break;
      }

      case "quit": {
        server.dispose();
        console.log("\n  Progress saved. See you next time! 👋\n");
        running = false;
        break;
      }
    }
  }
}
