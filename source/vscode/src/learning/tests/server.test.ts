// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { describe, it, before, after, beforeEach } from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { KatasServer, NoOpAIProvider } from "../server/index.js";

let server: KatasServer;
let workspacePath: string;

async function createServer(
  kataIds: string[] = ["getting_started"],
): Promise<KatasServer> {
  const s = new KatasServer();
  await s.initialize({
    kataIds,
    learningFilePath: join(workspacePath, "qdk-learning.json"),
    katasRoot: join(workspacePath, "quantum-katas"),
    katasRootRel: "./quantum-katas",
    aiProvider: new NoOpAIProvider(),
  });
  return s;
}

describe("KatasServer", () => {
  before(async () => {
    workspacePath = await mkdtemp(join(tmpdir(), "katas-test-"));
  });

  after(async () => {
    await rm(workspacePath, { recursive: true, force: true });
  });

  // ─── Catalog tests ───

  describe("catalog", () => {
    before(async () => {
      server = await createServer();
    });

    after(() => server.dispose());

    it("listKatas returns non-empty array with correct structure", () => {
      const katas = server.listKatas();
      assert.ok(katas.length > 0, "Should have at least one kata");
      const kata = katas[0];
      assert.ok(kata.id, "Kata should have an id");
      assert.ok(kata.title, "Kata should have a title");
      assert.equal(typeof kata.sectionCount, "number");
      assert.equal(typeof kata.completedCount, "number");
      assert.equal(typeof kata.recommended, "boolean");
    });

    it("getKataDetail returns sections", () => {
      const detail = server.getKataDetail("getting_started");
      assert.equal(detail.id, "getting_started");
      assert.ok(detail.title);
      assert.ok(detail.sections.length > 0, "Should have sections");
      for (const section of detail.sections) {
        assert.ok(
          section.type === "lesson" || section.type === "exercise",
          `Section type should be lesson or exercise, got ${section.type}`,
        );
        assert.ok(section.id, "Section should have an id");
        assert.ok(section.title, "Section should have a title");
        assert.equal(typeof section.isComplete, "boolean");
        assert.equal(typeof section.itemCount, "number");
      }
    });

    it("getKataDetail throws for invalid kata", () => {
      assert.throws(
        () => server.getKataDetail("nonexistent_kata"),
        /not found/i,
      );
    });
  });

  // ─── Workspace tests ───

  describe("workspace", () => {
    before(async () => {
      server = await createServer();
    });

    after(() => server.dispose());

    it("scaffolds exercise .qs files", async () => {
      // Getting started has at least one exercise (flip_qubit)
      const detail = server.getKataDetail("getting_started");
      const exercise = detail.sections.find((s) => s.type === "exercise");
      assert.ok(exercise, "Should have at least one exercise");

      server.goTo("getting_started", exercise!.id);
      const filePath = server.getExerciseFilePath();
      const content = await readFile(filePath, "utf-8");
      assert.ok(content.length > 0, "Exercise file should have content");
      assert.ok(
        content.includes("namespace Kata") ||
          content.includes("operation") ||
          content.includes("function"),
        "Exercise file should contain Q# code",
      );
    });

    it("readUserCode returns the file content", async () => {
      const detail = server.getKataDetail("getting_started");
      const exercise = detail.sections.find((s) => s.type === "exercise");
      server.goTo("getting_started", exercise!.id);
      const code = await server.readUserCode();
      assert.ok(code.length > 0);
    });

    it("does not overwrite existing exercise files on re-init", async () => {
      const detail = server.getKataDetail("getting_started");
      const exercise = detail.sections.find((s) => s.type === "exercise");
      server.goTo("getting_started", exercise!.id);
      const filePath = server.getExerciseFilePath();

      // Write custom content
      await writeFile(filePath, "// my custom solution\n", "utf-8");

      // Re-initialize
      const server2 = await createServer();
      const content = await readFile(filePath, "utf-8");
      assert.equal(
        content,
        "// my custom solution\n",
        "File should not be overwritten",
      );
      server2.dispose();
    });

    it("scaffolds example .qs files and exposes filePath on lesson-example items", async () => {
      // 'qubit' kata contains examples; use a fresh workspace so the test is hermetic.
      const wsBackup = workspacePath;
      workspacePath = await mkdtemp(join(tmpdir(), "katas-examples-"));
      const exampleServer = await createServer(["qubit"]);
      try {
        // Navigate to a lesson-example item.
        let pos = exampleServer.getPosition();
        let maxSteps = 100;
        while (pos.item.type !== "lesson-example" && maxSteps-- > 0) {
          const { moved } = exampleServer.next();
          if (!moved) break;
          pos = exampleServer.getPosition();
        }
        assert.equal(
          pos.item.type,
          "lesson-example",
          "Should find a lesson example in the qubit kata",
        );
        const example = pos.item as { filePath: string; code: string };
        assert.ok(example.filePath, "lesson-example should carry a filePath");
        assert.ok(
          example.filePath.includes(join("examples", "qubit")),
          `filePath should live under examples/qubit, got ${example.filePath}`,
        );
        const onDisk = await readFile(example.filePath, "utf-8");
        assert.equal(
          onDisk,
          example.code,
          "Scaffolded example file content should match the example's code",
        );
      } finally {
        exampleServer.dispose();
        await rm(workspacePath, { recursive: true, force: true });
        workspacePath = wsBackup;
      }
    });
  });

  // ─── Navigation tests ───

  describe("navigation", () => {
    beforeEach(async () => {
      server = await createServer();
      server.resetProgress();
    });

    it("getPosition returns initial position", () => {
      const pos = server.getPosition();
      assert.ok(pos.kataId, "Should have a kataId");
      assert.equal(typeof pos.sectionId, "string");
      assert.equal(typeof pos.itemIndex, "number");
      assert.ok(pos.item, "Should have a NavigationItem");
      assert.ok(pos.item.type, "NavigationItem should have a type");
    });

    it("next() advances through items", () => {
      const pos1 = server.getPosition();
      const next = server.next();
      assert.equal(next.moved, true, "Should be able to advance");

      const pos2 = server.getPosition();
      // Position should have changed
      const moved =
        pos2.kataId !== pos1.kataId ||
        pos2.sectionId !== pos1.sectionId ||
        pos2.itemIndex !== pos1.itemIndex;
      assert.ok(moved, "Position should change after next()");
    });

    it("previous() goes back", () => {
      server.next(); // move forward first
      const pos1 = server.getPosition();

      const prev = server.previous();
      assert.equal(prev.moved, true, "Should be able to go back");

      const pos2 = server.getPosition();
      const moved =
        pos1.kataId !== pos2.kataId ||
        pos1.sectionId !== pos2.sectionId ||
        pos1.itemIndex !== pos2.itemIndex;
      assert.ok(moved, "Position should change after previous()");
    });

    it("previous() reports moved=false at beginning", () => {
      const prev = server.previous();
      assert.equal(prev.moved, false, "Should not move past beginning");
    });

    it("next() reports moved=false at end", () => {
      // Navigate to the end
      let moved = true;
      let count = 0;
      while (moved) {
        moved = server.next().moved;
        count++;
        if (count > 1000) break; // safety
      }
      assert.equal(moved, false, "Should report moved=false at the end");
    });

    it("goTo() jumps to specific position", () => {
      const detail = server.getKataDetail("getting_started");
      const lastSection = detail.sections[detail.sections.length - 1];

      const state = server.goTo("getting_started", lastSection.id, 0);
      assert.ok(state.position, "Should return state with position");

      const pos = server.getPosition();
      assert.equal(pos.kataId, "getting_started");
      assert.equal(pos.sectionId, lastSection.id);
    });

    it("goTo() throws for invalid position", () => {
      assert.throws(
        () => server.goTo("getting_started", "nonexistent_section"),
        /not found/i,
      );
    });

    it("getAvailableActions returns exactly one primary binding", () => {
      const groups = server.getAvailableActions();
      const primaries = groups.flat().filter((b) => b.primary === true);
      assert.equal(
        primaries.length,
        1,
        "Exactly one binding should be marked primary",
      );
      assert.equal(primaries[0].key, "space");
    });

    it("every action binding has a label from the server", () => {
      const groups = server.getAvailableActions();
      for (const b of groups.flat()) {
        assert.ok(
          typeof b.label === "string" && b.label.length > 0,
          `Binding ${b.action} should have a non-empty label`,
        );
      }
    });

    it("getState returns position, actions, and progress consistent with granular getters", () => {
      const state = server.getState();
      assert.deepEqual(state.position, server.getPosition());
      assert.deepEqual(state.actions, server.getAvailableActions());
      // OverallProgress contains a Map; compare the concrete stats for simplicity.
      assert.deepEqual(state.progress.stats, server.getProgress().stats);
    });
  });

  // ─── Execution tests ───

  describe("execution", () => {
    before(async () => {
      // Re-create workspace to get fresh exercise files
      workspacePath = await mkdtemp(join(tmpdir(), "katas-exec-"));
      // Use 'qubit' kata which has both examples and exercises
      server = await createServer(["qubit"]);
    });

    after(async () => {
      server.dispose();
      await rm(workspacePath, { recursive: true, force: true });
    });

    it("run() on an example succeeds", async () => {
      // Navigate to find an example
      let pos = server.getPosition();
      let maxSteps = 50;
      while (pos.item.type !== "lesson-example" && maxSteps-- > 0) {
        const { moved } = server.next();
        if (!moved) break;
        pos = server.getPosition();
      }

      assert.equal(
        pos.item.type,
        "lesson-example",
        "Should find a lesson example",
      );
      const { result } = await server.run();
      assert.equal(result.success, true, "Example should run successfully");
      assert.ok(Array.isArray(result.events));
    });

    it("getCircuit() on an example returns circuit data", async () => {
      const pos = server.getPosition();
      assert.equal(pos.item.type, "lesson-example");
      const { result } = await server.getCircuit();
      assert.ok(result.circuit, "Should have circuit data");
      assert.ok(
        typeof result.ascii === "string",
        "Should have ASCII representation",
      );
    });

    it("run() on an exercise includes verification sources", async () => {
      // Navigate to find an exercise
      let pos = server.getPosition();
      let maxSteps = 50;
      while (pos.item.type !== "exercise" && maxSteps-- > 0) {
        const { moved } = server.next();
        if (!moved) break;
        pos = server.getPosition();
      }

      assert.equal(pos.item.type, "exercise", "Should find an exercise");
      // Run with placeholder code — should NOT get "entry point not found"
      const { result } = await server.run();
      assert.equal(typeof result.success, "boolean");
      // The key thing: it should not error with "entry point not found"
      if (result.error) {
        assert.ok(
          !result.error.includes("entry point not found"),
          `Should not get "entry point not found" error, got: ${result.error}`,
        );
      }
    });

    it("getCircuit() on an exercise does not error with 'entry point not found'", async () => {
      const pos = server.getPosition();
      assert.equal(pos.item.type, "exercise");
      try {
        const { result } = await server.getCircuit();
        assert.ok(result.circuit, "Should have circuit data");
        assert.ok(typeof result.ascii === "string");
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        assert.ok(
          !msg.includes("entry point not found"),
          `Should not get "entry point not found" error, got: ${msg}`,
        );
      }
    });
  });

  // ─── Exercise check tests ───

  describe("exercise checking", () => {
    before(async () => {
      workspacePath = await mkdtemp(join(tmpdir(), "katas-check-"));
      server = await createServer();
    });

    after(async () => {
      server.dispose();
      await rm(workspacePath, { recursive: true, force: true });
    });

    it("checkSolution() fails with placeholder code", async () => {
      // Navigate to exercise
      const detail = server.getKataDetail("getting_started");
      const exerciseSection = detail.sections.find(
        (s) => s.type === "exercise",
      );
      assert.ok(exerciseSection, "Should have an exercise");

      server.goTo("getting_started", exerciseSection!.id);
      const { result } = await server.checkSolution();
      assert.equal(result.passed, false, "Placeholder should fail");
    });

    it("checkSolution() passes with reference solution", async () => {
      // Get the reference solution and write it to the exercise file
      const detail = server.getKataDetail("getting_started");
      const exerciseSection = detail.sections.find(
        (s) => s.type === "exercise",
      );
      assert.ok(exerciseSection);

      server.goTo("getting_started", exerciseSection!.id);

      const solution = server.getFullSolution();
      assert.ok(solution.length > 0, "Should have a reference solution");

      // Write solution to file
      const filePath = server.getExerciseFilePath();
      await writeFile(filePath, solution, "utf-8");

      const { result } = await server.checkSolution();
      assert.equal(result.passed, true, "Reference solution should pass");
    });

    it("getNextHint() returns hints incrementally", () => {
      const detail = server.getKataDetail("getting_started");
      const exerciseSection = detail.sections.find(
        (s) => s.type === "exercise",
      );
      server.goTo("getting_started", exerciseSection!.id);
      const { result: hint } = server.getNextHint();
      assert.ok(hint, "Should have at least one hint");
      assert.equal(hint!.current, 1);
      assert.ok(hint!.total > 0);
      assert.ok(hint!.hint.length > 0, "Hint text should be non-empty");
    });

    it("getFullSolution() returns code", () => {
      const detail = server.getKataDetail("getting_started");
      const exerciseSection = detail.sections.find(
        (s) => s.type === "exercise",
      );
      server.goTo("getting_started", exerciseSection!.id);
      const solution = server.getFullSolution();
      assert.ok(solution.length > 0);
      assert.ok(
        solution.includes("X(q)") ||
          solution.includes("operation") ||
          solution.includes("namespace"),
        "Solution should contain Q# code",
      );
    });
  });

  // ─── Progress tests ───

  describe("progress", () => {
    before(async () => {
      workspacePath = await mkdtemp(join(tmpdir(), "katas-progress-"));
    });

    after(async () => {
      await rm(workspacePath, { recursive: true, force: true });
    });

    it("navigating past a lesson marks it complete", async () => {
      server = await createServer();
      // Navigate through all items in the first section (a lesson) until we cross into the next section
      const pos = server.getPosition();
      const startSection = pos.sectionId;
      let maxSteps = 50;
      while (maxSteps-- > 0) {
        const { moved } = server.next();
        if (!moved) break;
        const newPos = server.getPosition();
        if (newPos.sectionId !== startSection) break;
      }
      const progress = server.getProgress();
      const kp = progress.katas.get("getting_started");
      assert.ok(kp, "Should have progress for getting_started");
      assert.ok(kp!.completed > 0, "Should have completed sections");
      assert.ok(kp!.sections[0].isComplete, "First section should be complete");
      server.dispose();
    });

    it("progress persists across dispose + re-init", async () => {
      // First session — navigate through first section to complete it
      const s1 = await createServer();
      const startPos = s1.getPosition();
      const startSection = startPos.sectionId;
      let maxSteps = 50;
      while (maxSteps-- > 0) {
        const { moved } = s1.next();
        if (!moved) break;
        const newPos = s1.getPosition();
        if (newPos.sectionId !== startSection) break;
      }
      s1.dispose();

      // Wait for save
      await new Promise((r) => setTimeout(r, 200));

      // Second session
      const s2 = await createServer();
      const progress = s2.getProgress();
      const kp = progress.katas.get("getting_started");
      assert.ok(kp!.sections[0].isComplete, "Completion should persist");

      // Position should be restored
      const restoredPos = s2.getPosition();
      assert.equal(restoredPos.kataId, "getting_started");
      s2.dispose();
    });

    it("resetProgress clears everything", async () => {
      const s = await createServer();
      // Navigate through first section to mark it complete
      const pos = s.getPosition();
      const startSection = pos.sectionId;
      let maxSteps = 50;
      while (maxSteps-- > 0) {
        const { moved } = s.next();
        if (!moved) break;
        const newPos = s.getPosition();
        if (newPos.sectionId !== startSection) break;
      }
      s.resetProgress();
      const progress = s.getProgress();
      assert.equal(
        progress.stats.completedSections,
        0,
        "Should have zero completed",
      );
      s.dispose();
    });

    it("resetProgress with kataId clears only that kata", async () => {
      const s = await createServer(["getting_started"]);
      // Navigate through first section to mark it complete
      const pos = s.getPosition();
      const startSection = pos.sectionId;
      let maxSteps = 50;
      while (maxSteps-- > 0) {
        const { moved } = s.next();
        if (!moved) break;
        const newPos = s.getPosition();
        if (newPos.sectionId !== startSection) break;
      }
      s.resetProgress("getting_started");
      const progress = s.getProgress();
      const kp = progress.katas.get("getting_started")!;
      assert.equal(kp.completed, 0, "getting_started should be reset");
      s.dispose();
    });
  });

  // ─── AI tests ───

  describe("AI with NoOpProvider", () => {
    before(async () => {
      workspacePath = await mkdtemp(join(tmpdir(), "katas-ai-"));
      server = await createServer();
    });

    after(async () => {
      server.dispose();
      await rm(workspacePath, { recursive: true, force: true });
    });

    it("getAIHint returns null", async () => {
      // Navigate to exercise
      const detail = server.getKataDetail("getting_started");
      const exerciseSection = detail.sections.find(
        (s) => s.type === "exercise",
      );
      if (exerciseSection) {
        server.goTo("getting_started", exerciseSection.id);
        const { result: hint } = await server.getAIHint();
        assert.equal(hint, null);
      }
    });

    it("explainError returns null", async () => {
      const result = await server.explainError({
        code: "X(q);",
        error: "type mismatch",
      });
      assert.equal(result, null);
    });

    it("askConceptQuestion returns null", async () => {
      const { result } = await server.askConceptQuestion("What is a qubit?");
      assert.equal(result, null);
    });
  });
});
