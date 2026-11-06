import type { SteamStatus } from "../../domain/SteamStatus";

export interface StatusPollerOptions {
  read: () => Promise<SteamStatus>;
  /** False while an action runs or the window is hidden. */
  canPoll: () => boolean;
  /** Bumped by every action, so readings that started before it are dropped. */
  generation: () => number;
  onStatus: (status: SteamStatus) => void;
  onError: (error: unknown) => void;
}

/**
 * Reads Steam's status on an interval. The next read is scheduled only after
 * the previous one settles, so slow native calls never pile up.
 */
export class StatusPoller {
  private active = false;
  private pending = false;
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly options: StatusPollerOptions, private readonly interval = 3000) {}

  /** Reads now, then keeps reading every interval until stopped. */
  start(): void {
    this.active = true;
    void this.refresh();
  }

  /** Reads now, unless a read is already in flight. */
  async refresh(): Promise<void> {
    if (!this.active || this.pending) return;
    clearTimeout(this.timer);
    if (!this.options.canPoll()) {
      this.scheduleNext();
      return;
    }
    this.pending = true;
    const generation = this.options.generation();
    const stillCurrent = () => this.active && generation === this.options.generation() && this.options.canPoll();
    try {
      const next = await this.options.read();
      if (stillCurrent()) this.options.onStatus(next);
    } catch (error) {
      if (stillCurrent()) this.options.onError(error);
    } finally {
      this.pending = false;
      this.scheduleNext();
    }
  }

  stop(): void {
    this.active = false;
    clearTimeout(this.timer);
  }

  private scheduleNext(): void {
    if (this.active) this.timer = setTimeout(() => void this.refresh(), this.interval);
  }
}
