// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { assert } from "chai";
import { type ExtensionApi } from "../../src/extension";
import { activateExtension } from "../extensionUtils";

type LearningService = NonNullable<ExtensionApi["learning"]>;

suite("QDK Learning multi-course", function suite() {
  let service: LearningService;

  this.beforeAll(async function beforeAll() {
    const api = await activateExtension();
    // The learning feature is desktop-only, so this suite is skipped in the
    // web (browser) test host where no learning service is exposed.
    if (!api.learning) {
      this.skip();
    }
    service = api.learning!;
    await service.tryInitialize({ createIfMissing: true });
  });

  test("Katas is the default course", async () => {
    const courses = await service.getCourses();
    assert.isTrue(
      courses.some((c) => c.id === "katas"),
      "the built-in Katas course should always be available",
    );
    assert.equal(service.getActiveCourseId(), "katas");
  });

  test("Drop-in python-notebook course is discovered", async function test() {
    const courses = await service.getCourses();
    const descriptor = courses.find((c) => c.id === "circuit-diagrams");
    assert.ok(descriptor, "the fixture course should be discovered");
    assert.equal(descriptor!.kind, "python-notebook");
  });

  test("Notebook unit parses into a lesson, example, and two tasks", async function test() {
    await service.switchCourse("circuit-diagrams", "tree");
    try {
      assert.equal(service.getActiveCourseId(), "circuit-diagrams");
      const units = service.listUnits();
      assert.equal(units.length, 1, "course should have a single unit");

      const progress = service.getProgress();
      const activities = progress.units[0].activities;
      const ids = activities.map((a) => a.id);
      assert.include(ids, "build-bell");
      assert.include(ids, "display-circuit");

      const exercises = activities.filter((a) => a.type === "exercise");
      assert.equal(exercises.length, 2, "both tasks should become exercises");
    } finally {
      await service.switchCourse("katas", "tree");
    }
  });

  test("Environment check returns a structured report", async function test() {
    await service.switchCourse("circuit-diagrams", "tree");
    try {
      const report = await service.runEnvironmentCheck();
      assert.equal(report.courseId, "circuit-diagrams");
      assert.isAbove(
        report.checks.length,
        0,
        "the report should contain checks",
      );
      // Until the per-course environment is set up (or on a host without the
      // tooling), the report should flag problems and offer a fix.
      if (report.overallStatus !== "ok") {
        assert.isTrue(
          report.fixes.length > 0 ||
            report.checks.some((c) => c.status !== "ok"),
          "a failing report should be actionable",
        );
      }
    } finally {
      await service.switchCourse("katas", "tree");
    }
  });

  test("Katas course needs no environment (check passes)", async () => {
    await service.switchCourse("katas", "tree");
    const report = await service.runEnvironmentCheck();
    assert.equal(report.courseId, "katas");
    assert.equal(
      report.overallStatus,
      "ok",
      "Q# courses should pass diagnostics trivially",
    );
    assert.isFalse(report.fixes.some((r) => r.kind === "setup"));
  });
});
