import { expect, it } from "vitest";
import { RunningActions } from "./RunningActions";

it("allows the Steam and network pair, rejects duplicate or exclusive actions", () => {
  expect(RunningActions.none.canStart("restart")).toBe(true);
  expect(RunningActions.of("network").canStart("restart")).toBe(true);
  expect(RunningActions.of("restart").canStart("network")).toBe(true);
  expect(RunningActions.of("restart").canStart("restart")).toBe(false);
  expect(RunningActions.of("network").canStart("network")).toBe(false);
  for (const exclusive of ["path", "install-update", "check-update"] as const) {
    expect(RunningActions.of(exclusive).canStart("restart")).toBe(false);
    expect(RunningActions.of("restart", "network").canStart(exclusive)).toBe(false);
  }
});

it("keeps start order and leaves the original unchanged", () => {
  const network = RunningActions.none.with("network");
  const both = network.with("restart");
  expect(both.oldest).toBe("network");
  expect(network.has("restart")).toBe(false);
  expect(both.without("network").oldest).toBe("restart");
  expect(both.without("network").without("restart").isIdle).toBe(true);
});

it("tells the Steam controls apart from everything else", () => {
  expect(RunningActions.of("restart", "network").hasNonSteamAction).toBe(false);
  expect(RunningActions.of("check-update").hasNonSteamAction).toBe(true);
});
