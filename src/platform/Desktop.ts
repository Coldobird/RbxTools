import type { SteamStatus } from "../domain/SteamStatus";

export interface AvailableUpdate {
  readonly version: string;
}

/**
 * What the interface may ask of the computer. Every method rejects with a
 * `DesktopError`. Outside the live desktop app, reads return nothing real and
 * changes are refused.
 */
export interface Desktop {
  /** Running inside the desktop app (not a browser preview). */
  readonly isDesktop: boolean;
  /** Calls reach the real computer: the desktop app, outside the UI test view. */
  readonly isLive: boolean;
  readonly appVersion: string;

  readStatus(): Promise<SteamStatus>;
  /** Asks for steam.exe with a file dialog and saves it; `null` when cancelled. */
  chooseSteamExecutable(): Promise<SteamStatus | null>;
  /** Restarts Steam; `onLoadingUi` runs once Steam is up and its window is loading. */
  restartSteam(force: boolean, onLoadingUi?: () => void): Promise<void>;
  /** Pauses Steam's network by taking Steam offline, or resumes it. */
  setNetworkPaused(paused: boolean): Promise<SteamStatus>;
  /** The newer GitHub release, if any. */
  findUpdate(): Promise<AvailableUpdate | null>;
  /** Downloads and stages the update; the app exits to finish installing it. */
  installUpdate(): Promise<void>;
}
