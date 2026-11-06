import type { ActionName } from "../domain/RunningActions";
import type { SteamStatus } from "../domain/SteamStatus";
import type { AvailableUpdate } from "../platform/Desktop";
import { DesktopError } from "../platform/DesktopError";
import type { ActionHost, ActionScripts } from "./actions/ActionScripts";
import { LiveActions } from "./actions/LiveActions";
import { SimulatedActions } from "./actions/SimulatedActions";
import type { AppEnvironment } from "./AppEnvironment";
import { initialAppState, type AppState, type ModalName, type Page, type UpdateState } from "./AppState";
import { ErrorToast } from "./ErrorToast";
import { StartupUpdate } from "./live/StartupUpdate";
import { StatusMonitor } from "./live/StatusMonitor";
import { UpdateWatcher } from "./live/UpdateWatcher";
import { Store } from "./Store";
import { TestViewControls } from "./testview/TestViewControls";
import { Timeout } from "./Timeout";

const LOADED_VISIBLE_MS = 5_000;

/**
 * Owns the application state and every user action. React components read
 * the state through `subscribe`/`getSnapshot` and call methods to act.
 *
 * Each action is checked against the running ones here, then carried out by
 * the live scripts on the desktop or by simulated ones elsewhere.
 */
export class AppController {
  /** The UI test view panel. */
  readonly testViewControls: TestViewControls;

  private readonly store: Store<AppState>;
  private readonly live: ActionScripts;
  private readonly simulated: ActionScripts;
  private readonly errorToast = new ErrorToast(error => this.store.update({ error }));
  private readonly startupUpdate = new StartupUpdate();
  private readonly loadedTimeout = new Timeout();
  /** Bumped by every restart, so network results can tell a session changed. */
  private restartGeneration = 0;
  /** Bumped by every action, so status readings that started earlier are dropped. */
  private statusGeneration = 0;
  private monitor: StatusMonitor | null = null;
  private updates: UpdateWatcher | null = null;

  constructor(private readonly environment: AppEnvironment) {
    const { desktop, testView } = environment;
    this.store = new Store(initialAppState(testView.isActive, desktop.isLive));
    const host = this.actionHost();
    this.live = new LiveActions(desktop, host);
    this.simulated = new SimulatedActions(host);
    this.testViewControls = new TestViewControls({
      snapshot: () => this.state,
      simulate: change => this.store.update(change),
    });
  }

  get appVersion(): string {
    return this.environment.desktop.appVersion;
  }

  get isDesktop(): boolean {
    return this.environment.desktop.isDesktop;
  }

  readonly subscribe = (listener: () => void): (() => void) => this.store.subscribe(listener);
  readonly getSnapshot = (): AppState => this.store.getSnapshot();

  // ── Lifecycle ────────────────────────────────────────────────────────────

  /** Starts live status polling and update checks, when connected to the desktop. */
  start(): void {
    this.stop();
    this.environment.timings.markOnce("app.mounted");
    if (!this.environment.desktop.isLive) {
      this.store.update({ loading: false });
      return;
    }
    this.monitor = new StatusMonitor(this.environment, {
      isBusy: () => !this.state.running.isIdle,
      statusGeneration: () => this.statusGeneration,
      updateStatus: change => this.updateStatus(change),
      statusSettled: () => this.store.update({ loading: false }),
      reportStatusError: message => this.showError(message),
    });
    this.updates = new UpdateWatcher(this.environment.desktop, {
      updateChecked: (available, startup) => this.updateChecked(available, startup),
      updateCheckFailed: message => this.setUpdate({ error: message }),
    });
    this.monitor.start();
    this.updates.start();
  }

  stop(): void {
    this.monitor?.stop();
    this.updates?.stop();
    this.monitor = null;
    this.updates = null;
  }

  // ── Navigation ───────────────────────────────────────────────────────────

  showPage(page: Page): void {
    this.store.update({ page });
  }

  openModal(modal: ModalName): void {
    this.store.update({ modal });
  }

  closeModal(): void {
    this.store.update({ modal: null });
    this.showStartupUpdateWhenClear();
  }

  /** Backdrop click or Escape. A forced restart and a running install must be answered explicitly. */
  dismissModal(): void {
    if (this.canDismissModal) this.closeModal();
  }

  get canDismissModal(): boolean {
    return this.state.modal !== "force" && this.canCloseModal;
  }

  /** A running install keeps its dialog open. */
  get canCloseModal(): boolean {
    return !this.state.running.has("install-update");
  }

  dismissError(): void {
    this.showError(null);
  }

  dismissNotice(): void {
    this.store.update({ notice: null });
  }

  /** Enters or leaves the UI test view, starting over in the new mode. */
  toggleTestView(): void {
    if (!this.state.running.isIdle) return;
    const testView = !this.state.testView;
    this.environment.testView.set(testView);
    this.stop();
    this.errorToast.cancel();
    this.loadedTimeout.clear();
    this.startupUpdate.drop();
    this.testViewControls.configure({ panelOpen: true });
    this.store.replace(initialAppState(testView, this.environment.desktop.isLive));
    this.start();
  }

  // ── Actions ──────────────────────────────────────────────────────────────

  restartSteam(force = false): void {
    if (!this.state.running.canStart("restart")) return;
    this.restartGeneration += 1;
    this.loadedTimeout.clear();
    this.store.update({ modal: null, restartPhase: "restarting" });
    void this.run("restart", () => this.scripts.restartSteam(force));
  }

  /** Pauses Steam's network, or resumes it. */
  toggleNetwork(): void {
    const resume = this.state.status.networkPaused;
    const request = {
      pause: !resume,
      restartGeneration: this.restartGeneration,
      resumedAt: resume ? Date.now() : undefined,
    };
    void this.run("network", () => this.scripts.setNetworkPaused(request));
  }

  chooseSteamExecutable(): void {
    void this.run("path", () => this.scripts.chooseSteamExecutable());
  }

  checkForUpdate(): void {
    void this.run("check-update", () => this.recordingUpdateErrors(() => this.scripts.checkForUpdate()));
  }

  installUpdate(): void {
    void this.run("install-update", () => {
      this.setUpdate({ error: null, feedback: null });
      return this.recordingUpdateErrors(() => this.scripts.installUpdate());
    });
  }

  /** The Settings update button: installs a found update, or looks for one. */
  updateFromSettings(): void {
    if (this.state.update.available) this.installUpdate();
    else this.checkForUpdate();
  }

  // ── Internals ────────────────────────────────────────────────────────────

  private get state(): AppState {
    return this.store.getSnapshot();
  }

  private get scripts(): ActionScripts {
    return this.environment.desktop.isLive ? this.live : this.simulated;
  }

  /** Runs an action if the running ones allow it, and reports its failure. */
  private async run(name: ActionName, work: () => Promise<void>): Promise<void> {
    if (!this.state.running.canStart(name)) return;
    this.statusGeneration += 1;
    this.showError(null);
    this.store.update({ running: this.state.running.with(name), notice: null });
    try {
      await work();
    } catch (error) {
      this.reportFailure(name, DesktopError.from(error));
    } finally {
      this.store.update({ running: this.state.running.without(name) });
      this.showStartupUpdateWhenClear();
    }
  }

  private reportFailure(name: ActionName, error: DesktopError): void {
    if (name === "restart") this.store.update({ restartPhase: null });
    if (error.kind === "steam-still-running") {
      this.openModal("force");
      return;
    }
    // Update errors appear in Settings or the update dialog instead of a toast.
    const shownElsewhere = name === "check-update" || (name === "install-update" && this.state.modal !== null);
    if (!shownElsewhere) this.showError(error.message);
  }

  private async recordingUpdateErrors(work: () => Promise<void>): Promise<void> {
    try {
      await work();
    } catch (error) {
      this.setUpdate({ error: DesktopError.from(error).message });
      throw error;
    }
  }

  /** A startup update opens a dialog; a later one shows a toast with an Update button. */
  private updateChecked(available: AvailableUpdate | null, startup: boolean): void {
    const announce = available && !startup && this.state.running.isIdle;
    this.store.update({
      update: { available, error: null, feedback: null },
      ...(announce ? { notice: { message: `RBX Tools v${available.version} is available.`, offersUpdate: true } } : {}),
    });
    if (available && startup) {
      this.startupUpdate.hold();
      this.showStartupUpdateWhenClear();
    }
  }

  private showStartupUpdateWhenClear(): void {
    const clear = !this.state.modal && this.state.running.isIdle;
    if (this.startupUpdate.release(clear) && this.state.update.available) this.openModal("update");
  }

  /** Shows an error toast, or clears it; a repeated message keeps its original timer. */
  private showError(message: string | null): void {
    if (message === this.state.error) return;
    this.errorToast.show(message, { stays: this.state.testView });
  }

  /** The narrow access the action scripts get. */
  private actionHost(): ActionHost {
    return {
      isTestView: () => this.state.testView,
      updateStatus: change => this.updateStatus(change),
      setRestartPhase: restartPhase => this.store.update({ restartPhase }),
      showLoadedBriefly: () => this.loadedTimeout.start(LOADED_VISIBLE_MS, () => this.store.update({ restartPhase: null })),
      setUpdate: change => this.setUpdate(change),
      closeModal: modal => { if (this.state.modal === modal) this.closeModal(); },
      restartedSince: generation => this.state.running.has("restart") || generation !== this.restartGeneration,
    };
  }

  private updateStatus(change: (status: SteamStatus) => SteamStatus): void {
    this.store.update({ status: change(this.state.status) });
  }

  private setUpdate(change: Partial<UpdateState>): void {
    this.store.update({ update: { ...this.state.update, ...change } });
  }
}
