import { ConnectionGrace } from "../../domain/ConnectionGrace";
import type { SteamStatus } from "../../domain/SteamStatus";
import { DesktopError } from "../../platform/DesktopError";
import type { AppEnvironment } from "../AppEnvironment";
import { StatusPoller } from "./StatusPoller";

export interface StatusMonitorHost {
  isBusy(): boolean;
  statusGeneration(): number;
  /** Applies a change to the current status. */
  updateStatus(change: (current: SteamStatus) => SteamStatus): void;
  /** A reading (or failure) arrived. */
  statusSettled(): void;
  /** A reading failed after the grace period expired. */
  reportStatusError(message: string): void;
}

/**
 * Keeps the live Steam status current while the window is visible. Missing
 * readings keep the last confirmed connection for the grace period.
 */
export class StatusMonitor {
  private readonly grace = new ConnectionGrace();
  private readonly poller: StatusPoller;
  private stopWatchingVisibility = (): void => {};

  constructor(
    private readonly environment: Pick<AppEnvironment, "desktop" | "visibility" | "timings">,
    private readonly host: StatusMonitorHost,
  ) {
    const { desktop, visibility } = environment;
    this.poller = new StatusPoller({
      read: () => desktop.readStatus(),
      canPoll: () => !host.isBusy() && !visibility.isHidden,
      generation: () => host.statusGeneration(),
      onStatus: next => this.applyReading(next),
      onError: error => this.applyFailure(error),
    });
  }

  start(): void {
    this.stopWatchingVisibility = this.environment.visibility.onVisible(() => this.resume());
    this.poller.start();
  }

  stop(): void {
    this.poller.stop();
    this.stopWatchingVisibility();
  }

  private applyReading(next: SteamStatus): void {
    this.environment.timings.markOnce("app.status.ready");
    const expired = this.grace.observe(next);
    this.host.updateStatus(current => current.mergePolled(next, expired));
    this.host.statusSettled();
  }

  private applyFailure(error: unknown): void {
    const expired = this.grace.observe();
    this.host.updateStatus(current => current.steamRunning && !current.networkPaused
      ? current.mergePolled(current.with({ steamOnline: null }), expired)
      : current);
    if (expired) this.host.reportStatusError(DesktopError.from(error).message);
    this.host.statusSettled();
  }

  /** Time spent hidden never ages a retained reading. */
  private resume(): void {
    this.grace.reset();
    this.host.updateStatus(current => current.with({ connectionStale: false }));
    void this.poller.refresh();
  }
}
