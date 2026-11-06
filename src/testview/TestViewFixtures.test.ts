import { expect, it } from "vitest";
import { TestViewFixtures, testViewSteamStates } from "./TestViewFixtures";

it("represents disconnected and unknown status without claiming online", () => {
  expect(TestViewFixtures.steamStatus("online").steamOnline).toBe(true);
  expect(TestViewFixtures.steamStatus("offline").steamOnline).toBe(false);
  expect(TestViewFixtures.steamStatus("connecting").steamOnline).toBeNull();
  expect(TestViewFixtures.steamStatus("paused")).toMatchObject({ networkPaused: true, steamOnline: false });
  expect(TestViewFixtures.steamStatus("stopped").steamRunning).toBe(false);
  expect(TestViewFixtures.steamStatus("missing").steamPath).toBeNull();
});

it("describes each simulated status as the state that produced it", () => {
  for (const state of testViewSteamStates) {
    expect(TestViewFixtures.steamStateOf(TestViewFixtures.steamStatus(state))).toBe(state);
  }
});
