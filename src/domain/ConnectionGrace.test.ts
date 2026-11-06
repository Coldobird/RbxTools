import { expect, it } from "vitest";
import { ConnectionGrace } from "./ConnectionGrace";
import { SteamStatus } from "./SteamStatus";

/** A grace period driven by a hand-moved clock. */
class ManualClock {
  now = 0;
  readonly grace = new ConnectionGrace(() => this.now);
}

const confirmed = SteamStatus.from({ steamPath: "C:\\Steam\\steam.exe", steamRunning: true, steamOnline: true, networkPaused: false });
const missing = confirmed.with({ steamOnline: null });

it("expires 30 seconds after the last definite reading", () => {
  const clock = new ManualClock();
  clock.now = 29_999;
  expect(clock.grace.observe(missing)).toBe(false);
  clock.now = 30_000;
  expect(clock.grace.observe()).toBe(true);
  expect(clock.grace.observe(confirmed)).toBe(false);
});

it("gives resume a fresh grace period", () => {
  const clock = new ManualClock();
  clock.now = 120_000;
  clock.grace.reset();
  expect(clock.grace.observe(missing)).toBe(false);
  clock.now += 29_999;
  expect(clock.grace.observe(missing)).toBe(false);
  clock.now++;
  expect(clock.grace.observe(missing)).toBe(true);
});

it("counts stopped and paused readings as definite", () => {
  for (const reading of [missing.with({ steamRunning: false }), missing.with({ networkPaused: true })]) {
    const clock = new ManualClock();
    clock.now = 30_000;
    expect(clock.grace.observe(reading)).toBe(false);
  }
});
