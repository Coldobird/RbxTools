import { CONNECTION_GRACE_MS, type SteamStatus } from "./SteamStatus";

/**
 * Tracks how long Steam has gone without a definite connection reading.
 * Reset on resume, so time spent minimized never ages a retained reading.
 */
export class ConnectionGrace {
  private lastResponse: number;

  constructor(private readonly now: () => number = Date.now) {
    this.lastResponse = now();
  }

  reset(): void {
    this.lastResponse = this.now();
  }

  /** Records a reading (or a failed one) and reports whether the grace period has expired. */
  observe(status?: SteamStatus): boolean {
    if (status && (status.steamOnline !== null || !status.steamRunning || status.networkPaused)) {
      this.lastResponse = this.now();
    }
    return this.now() - this.lastResponse >= CONNECTION_GRACE_MS;
  }
}
