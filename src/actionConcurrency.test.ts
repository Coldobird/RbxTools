import { expect, it } from "vitest";
import { canStartAction } from "./actionConcurrency";

it("allows the Steam and network pair, rejects duplicate or exclusive actions", () => {
  expect(canStartAction(new Set(), "restart")).toBe(true);
  expect(canStartAction(new Set(["network"]), "restart")).toBe(true);
  expect(canStartAction(new Set(["restart"]), "network")).toBe(true);
  expect(canStartAction(new Set(["restart"]), "restart")).toBe(false);
  expect(canStartAction(new Set(["network"]), "network")).toBe(false);
  for (const exclusive of ["path", "privacy", "install-update", "check-update"]) {
    expect(canStartAction(new Set([exclusive]), "restart")).toBe(false);
    expect(canStartAction(new Set(["restart", "network"]), exclusive)).toBe(false);
  }
});
