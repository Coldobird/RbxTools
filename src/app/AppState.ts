import { RunningActions } from "../domain/RunningActions";
import { SteamStatus } from "../domain/SteamStatus";
import type { AvailableUpdate } from "../platform/Desktop";
import { TestViewFixtures } from "../testview/TestViewFixtures";

export type Page = "library" | "steamy";
export type ModalName = "settings" | "update" | "force";
/** A restart's progress: Steam closing and starting, its window loading, then loaded. */
export type RestartPhase = "restarting" | "loading-ui" | "loaded";

export interface Notice {
  readonly message: string;
  /** The notice announces an update: it carries an Update button and stays until dismissed. */
  readonly offersUpdate: boolean;
}

export interface UpdateState {
  readonly available: AvailableUpdate | null;
  readonly error: string | null;
  /** A short result line, such as "RBX Tools is up to date." */
  readonly feedback: string | null;
}

/** Everything the interface renders, as one immutable snapshot. */
export interface AppState {
  readonly testView: boolean;
  readonly page: Page;
  readonly status: SteamStatus;
  /** The first live status reading has not arrived yet. */
  readonly loading: boolean;
  readonly running: RunningActions;
  readonly error: string | null;
  readonly notice: Notice | null;
  readonly update: UpdateState;
  readonly modal: ModalName | null;
  readonly restartPhase: RestartPhase | null;
}

/** The state a session starts in: live, browser preview, or UI test view. */
export function initialAppState(testView: boolean, live: boolean): AppState {
  return {
    testView,
    page: "library",
    status: testView ? TestViewFixtures.steamStatus("online") : SteamStatus.notDetected,
    loading: live,
    running: RunningActions.none,
    error: null,
    notice: null,
    update: { available: testView ? TestViewFixtures.update : null, error: null, feedback: null },
    modal: null,
    restartPhase: null,
  };
}
