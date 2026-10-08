import type { Status } from "./api";

export function isUiTestRequested(search: string): boolean {
  return new URLSearchParams(search).get("ui-test") === "1";
}

let active = isUiTestRequested(typeof window === "undefined" ? "" : window.location.search);

export function isUiTestActive(): boolean { return active; }
export function setUiTestActive(next: boolean): void { active = next; }
export function requireLiveUi(): void {
  if (active) throw new Error("System actions are disabled in UI test view.");
}

export const testUpdateVersion = "99.0.0";
export const testSteamStates = ["online", "offline", "connecting", "blocked", "stopped", "missing"] as const;
export type TestSteamState = typeof testSteamStates[number];

export function testSteamStatus(state: TestSteamState): Status {
  const path = state === "missing" ? null : "C:\\Preview\\Steam\\steam.exe";
  return {
    steamPath: path,
    targetPath: path,
    steamRunning: state !== "stopped" && state !== "missing",
    steamOnline: state === "connecting" ? null : state === "online",
    blocked: state === "blocked",
    elevated: true,
  };
}
