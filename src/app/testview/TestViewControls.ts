import { RunningActions } from "../../domain/RunningActions";
import { TestViewFixtures, type TestViewSteamState } from "../../testview/TestViewFixtures";
import { defaultTestViewPanel, type TestViewPanelSettings } from "../../testview/TestViewPanelSettings";
import type { AppState, ModalName } from "../AppState";
import { Store } from "../Store";

export type ToastChoice = "none" | "success" | "error" | "update" | "stacked";
export type ModalChoice = ModalName | "none";
export type BusyChoice = "none" | "status" | "restart" | "confirm-startup" | "loaded" | "network" | "check-update" | "install-update";
export type UpdateChoice = "available" | "current" | "error";

/** The parts of the app state the UI test view may simulate. */
export type TestViewSimulation = Partial<Pick<AppState,
  "status" | "loading" | "running" | "restartPhase" | "error" | "notice" | "update" | "modal">>;

/** What the test view controls read and change. */
export interface TestViewHost {
  snapshot(): AppState;
  simulate(change: TestViewSimulation): void;
}

/**
 * The UI test view panel: its own settings, and controls that put the real
 * interface into simulated states for inspection. Nothing here reaches the
 * computer.
 */
export class TestViewControls {
  /** The panel's own settings, apart from the app's state. */
  readonly settings = new Store<TestViewPanelSettings>(defaultTestViewPanel);

  constructor(private readonly host: TestViewHost) {}

  configure(change: Partial<TestViewPanelSettings>): void {
    this.settings.update(change);
  }

  /** The simulated Steam state that matches the current status. */
  get steamState(): TestViewSteamState {
    return TestViewFixtures.steamStateOf(this.state.status);
  }

  simulateSteam(steamState: TestViewSteamState): void {
    this.host.simulate({ status: TestViewFixtures.steamStatus(steamState), loading: false, restartPhase: null });
  }

  get toast(): ToastChoice {
    const { error, notice } = this.state;
    if (error && notice) return "stacked";
    if (error) return "error";
    if (notice) return notice.offersUpdate ? "update" : "success";
    return "none";
  }

  simulateToast(choice: ToastChoice): void {
    const withError = choice === "error" || choice === "stacked";
    const withUpdate = choice === "update" || choice === "stacked";
    const { update } = TestViewFixtures;
    this.host.simulate({
      error: withError ? "Steam could not reconnect. Check your connection and try again." : null,
      notice: withUpdate ? { message: `RBX Tools v${update.version} is available.`, offersUpdate: true }
        : choice === "success" ? { message: "Steam network access is restored.", offersUpdate: false }
        : null,
      ...(withUpdate ? { update: { ...this.state.update, available: update } } : {}),
    });
  }

  get modal(): ModalChoice {
    return this.state.modal ?? "none";
  }

  simulateModal(choice: ModalChoice): void {
    this.host.simulate({
      modal: choice === "none" ? null : choice,
      ...(choice === "update" ? { update: { ...this.state.update, available: TestViewFixtures.update } } : {}),
    });
  }

  get busy(): BusyChoice {
    const { restartPhase, running, loading } = this.state;
    if (restartPhase === "loading-ui") return "confirm-startup";
    const oldest = running.oldest;
    if (oldest && oldest !== "path") return oldest;
    if (loading) return "status";
    return restartPhase === "loaded" ? "loaded" : "none";
  }

  simulateBusy(choice: BusyChoice): void {
    const idle = choice === "none" || choice === "status" || choice === "loaded";
    this.host.simulate({
      running: idle ? RunningActions.none : RunningActions.of(choice === "confirm-startup" ? "restart" : choice),
      loading: choice === "status",
      restartPhase: choice === "restart" ? "restarting"
        : choice === "confirm-startup" ? "loading-ui"
        : choice === "loaded" ? "loaded"
        : null,
      ...(choice === "loaded" ? { status: TestViewFixtures.steamStatus("online") } : {}),
    });
  }

  get update(): UpdateChoice {
    const { update } = this.state;
    if (update.error) return "error";
    return update.available ? "available" : "current";
  }

  simulateUpdate(choice: UpdateChoice): void {
    this.host.simulate({
      update: {
        available: choice === "current" ? null : TestViewFixtures.update,
        error: choice === "error" ? "GitHub could not be reached. Try again." : null,
        feedback: choice === "current" ? "RBX Tools is up to date." : null,
      },
    });
  }

  private get state(): AppState {
    return this.host.snapshot();
  }
}
