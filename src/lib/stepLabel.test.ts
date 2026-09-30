import { describe, expect, it } from "bun:test";

import { currentStepLabel } from "./stepLabel";

describe("currentStepLabel", () => {
  it("names the step the turn is waiting on", () => {
    const steps = [
      { label: "Read src/lib/ipc.ts" },
      { label: "$ bun test", live: true },
    ];

    expect(currentStepLabel(steps)).toBe("$ bun test");
  });

  it("takes the last live step, not the first", () => {
    const steps = [
      { label: "$ bun test", live: true },
      { label: "Read src/lib/ipc.ts" },
      { label: "Running grep", live: true },
    ];

    expect(currentStepLabel(steps)).toBe("Running grep");
  });

  it("falls back to the newest step when none is live", () => {
    // Between two tools the previous one can finish before the next is
    // recorded. A stale name beats an empty header.
    const steps = [
      { label: "Read src/lib/ipc.ts" },
      { label: "$ bun test" },
    ];

    expect(currentStepLabel(steps)).toBe("$ bun test");
  });

  it("treats a missing live flag as not live", () => {
    expect(currentStepLabel([{ label: "one" }, { label: "two", live: false }])).toBe(
      "two",
    );
  });

  it("returns empty when there is nothing to name", () => {
    expect(currentStepLabel([])).toBe("");
  });
});
