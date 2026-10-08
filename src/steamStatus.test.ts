import { expect, it } from "vitest";
import type { Status } from "./api";
import { connectionLabel } from "./steamStatus";

const status: Status = { steamPath: "C:\\Steam\\steam.exe", targetPath: "C:\\Steam\\steam.exe", steamRunning: true, steamOnline: false, blocked: false, elevated: true };

it("does not claim online just because Steam is running or a block was removed", () => {
  expect(connectionLabel(status)).toBe("Steam offline");
  expect(connectionLabel({ ...status, steamOnline: null })).toBe("Connecting…");
  expect(connectionLabel({ ...status, steamOnline: true })).toBe("Steam online");
  expect(connectionLabel({ ...status, steamOnline: true, blocked: true })).toContain("blocked");
  expect(connectionLabel({ ...status, steamOnline: true, steamRunning: false })).toBe("Steam is not running");
});
