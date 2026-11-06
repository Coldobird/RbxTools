import { expect, it } from "vitest";
import { TestViewMode } from "./TestViewMode";

it("requires an explicit activation parameter", () => {
  expect(TestViewMode.requestedBy("?ui-test=1")).toBe(true);
  expect(TestViewMode.requestedBy("?other=1&ui-test=1")).toBe(true);
  expect(TestViewMode.requestedBy("?ui-test=0")).toBe(false);
  expect(TestViewMode.requestedBy("?other=ui-test=1")).toBe(false);
  expect(TestViewMode.requestedBy("")).toBe(false);
});

it("switches on and off", () => {
  const mode = new TestViewMode(true);
  mode.set(false);
  expect(mode.isActive).toBe(false);
});
