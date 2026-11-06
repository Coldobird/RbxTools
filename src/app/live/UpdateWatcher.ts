import type { AvailableUpdate, Desktop } from "../../platform/Desktop";
import { DesktopError } from "../../platform/DesktopError";

const RECHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

export interface UpdateWatcherHost {
  /** A check finished; `startup` is true for the check made when the app opened. */
  updateChecked(update: AvailableUpdate | null, startup: boolean): void;
  updateCheckFailed(message: string): void;
}

/** Checks GitHub for a newer release on startup and every six hours. */
export class UpdateWatcher {
  private active = false;
  private interval: ReturnType<typeof setInterval> | undefined;

  constructor(private readonly desktop: Desktop, private readonly host: UpdateWatcherHost) {}

  start(): void {
    this.active = true;
    this.check(true);
    this.interval = setInterval(() => this.check(false), RECHECK_INTERVAL_MS);
  }

  stop(): void {
    this.active = false;
    clearInterval(this.interval);
  }

  private check(startup: boolean): void {
    this.desktop.findUpdate().then(
      update => { if (this.active) this.host.updateChecked(update, startup); },
      error => { if (this.active) this.host.updateCheckFailed(DesktopError.from(error).message); },
    );
  }
}
