import { SteamStatus } from "../domain/SteamStatus";

/** The Steam states the UI test view can simulate, in display order. */
export const testViewSteamStates = [
  "online", "offline", "online-last-confirmed", "offline-last-confirmed",
  "connecting", "paused", "stopped", "missing",
] as const;
export type TestViewSteamState = typeof testViewSteamStates[number];

/** The simulated data the UI test view and the browser preview show. */
export class TestViewFixtures {
  static readonly steamPath = "C:\\Preview\\Steam\\steam.exe";
  static readonly update = { version: "99.0.0" } as const;

  /** A Steam reading that looks like `state`. */
  static steamStatus(state: TestViewSteamState): SteamStatus {
    const lastConfirmed = state === "online-last-confirmed" || state === "offline-last-confirmed";
    return SteamStatus.from({
      steamPath: state === "missing" ? null : TestViewFixtures.steamPath,
      steamRunning: state !== "stopped" && state !== "missing",
      steamOnline: state === "connecting" ? null : state === "online" || state === "online-last-confirmed",
      networkPaused: state === "paused",
    }).with({ connectionStale: lastConfirmed });
  }

  /** The simulated state that best describes `status`. */
  static steamStateOf(status: SteamStatus): TestViewSteamState {
    if (!status.steamPath) return "missing";
    if (!status.steamRunning) return "stopped";
    if (status.networkPaused) return "paused";
    if (status.steamOnline === null) return "connecting";
    if (status.connectionStale) return status.steamOnline ? "online-last-confirmed" : "offline-last-confirmed";
    return status.steamOnline ? "online" : "offline";
  }
}
