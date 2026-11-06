import type { SteamStatus } from "../../domain/SteamStatus";
import type { ModalName, RestartPhase, UpdateState } from "../AppState";

/** What an action script may change while it runs: only what actions own. */
export interface ActionHost {
  isTestView(): boolean;
  updateStatus(change: (status: SteamStatus) => SteamStatus): void;
  setRestartPhase(phase: RestartPhase): void;
  /** Shows "Loaded" for a few seconds after a restart. */
  showLoadedBriefly(): void;
  setUpdate(change: Partial<UpdateState>): void;
  /** Closes `modal` if it is the one open. */
  closeModal(modal: ModalName): void;
  /** Whether Steam restarted, or is restarting, since `generation` was taken. */
  restartedSince(generation: number): boolean;
}

/** A network pause or resume, as requested when the button was pressed. */
export interface NetworkRequest {
  readonly pause: boolean;
  /** The restart generation when the button was pressed. */
  readonly restartGeneration: number;
  /** When a resume was requested; drives the "Restoring connection…" label. */
  readonly resumedAt?: number;
}

/**
 * The work behind each user action, after the controller has decided it may
 * run. One implementation talks to the computer; the other simulates it for
 * the browser preview and the UI test view.
 */
export interface ActionScripts {
  restartSteam(force: boolean): Promise<void>;
  setNetworkPaused(request: NetworkRequest): Promise<void>;
  chooseSteamExecutable(): Promise<void>;
  checkForUpdate(): Promise<void>;
  installUpdate(): Promise<void>;
}
