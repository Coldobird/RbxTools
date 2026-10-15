import { expect, it } from "vitest";
import type { Status } from "./api";
import { connectionLabel } from "./steamStatus";

const status: Status = { steamPath: "C:\\Steam\\steam.exe", targetPath: "C:\\Steam\\steam.exe", steamRunning: true, steamOnline: false, blocked: false, elevated: true };

it("shows connecting during the grace period and restoring during recovery", () => {
  expect(connectionLabel({ ...status, steamOnline: null, connectionPending: true })).toBe("Connecting…");
  expect(connectionLabel({ ...status, steamOnline: null, restoringSince: 0 })).toBe("Restoring connection…");
});

it("does not claim online just because Steam is running or a block was removed", () => {
  expect(connectionLabel(status)).toBe("Steam offline");
  expect(connectionLabel({ ...status, steamOnline: null })).toBe("Steam connection unavailable");
  expect(connectionLabel({ ...status, steamOnline: true })).toBe("Steam online");
  expect(connectionLabel({ ...status, steamOnline: true, blocked: true })).toBe("Steam offline");
  expect(connectionLabel({ ...status, steamOnline: true, steamRunning: false })).toBe("Steam is not running");
});

it("identifies retained connection readings without overriding current blocked or stopped states", () => {
  const stale = { ...status, connectionStale: true };
  expect(connectionLabel({ ...stale, steamOnline: true })).toBe("Last confirmed online");
  expect(connectionLabel(stale)).toBe("Last confirmed offline");
  expect(connectionLabel({ ...stale, steamOnline: null })).toBe("Steam connection unavailable");
  expect(connectionLabel({ ...stale, blocked: true })).toBe("Steam offline");
  expect(connectionLabel({ ...stale, steamRunning: false })).toBe("Steam is not running");
  expect(connectionLabel({ ...stale, steamPath: null })).toBe("Steam location needed");
});
