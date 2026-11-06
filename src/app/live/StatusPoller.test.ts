import { afterEach, describe, expect, it, vi } from "vitest";
import { SteamStatus } from "../../domain/SteamStatus";
import { StatusPoller } from "./StatusPoller";

const status = SteamStatus.from({ steamPath: "C:\\Steam\\steam.exe", steamRunning: true, steamOnline: true, networkPaused: false });

/** A poller wired to controllable busy and generation signals. */
class PollerHarness {
  busy = false;
  generation = 0;
  readonly onStatus = vi.fn();
  readonly onError = vi.fn();
  readonly poller: StatusPoller;

  constructor(read: () => Promise<SteamStatus> = async () => status) {
    this.poller = new StatusPoller({
      read,
      canPoll: () => !this.busy,
      generation: () => this.generation,
      onStatus: this.onStatus,
      onError: this.onError,
    });
    this.poller.start();
  }

  /** Simulates an action starting, which makes in-flight readings stale. */
  invalidate(): void {
    this.generation++;
  }
}

/** A read whose result the test settles by hand. */
class PendingRead {
  resolve!: (status: SteamStatus) => void;
  reject!: (error: Error) => void;
  readonly read = vi.fn(() => new Promise<SteamStatus>((resolve, reject) => {
    this.resolve = resolve;
    this.reject = reject;
  }));
}

afterEach(() => vi.useRealTimers());

describe("status polling", () => {
  it("does not overlap slow requests and stops after cleanup", async () => {
    vi.useFakeTimers();
    const pending = new PendingRead();
    const harness = new PollerHarness(pending.read);
    await vi.advanceTimersByTimeAsync(12000);
    void harness.poller.refresh();
    expect(pending.read).toHaveBeenCalledTimes(1);
    pending.resolve(status);
    await vi.advanceTimersByTimeAsync(3000);
    expect(pending.read).toHaveBeenCalledTimes(2);
    harness.poller.stop();
    pending.resolve(status);
    await vi.advanceTimersByTimeAsync(12000);
    expect(pending.read).toHaveBeenCalledTimes(2);
    expect(harness.onStatus).toHaveBeenCalledTimes(1);
  });

  it("ignores responses and errors from before an action", async () => {
    vi.useFakeTimers();
    const pending = new PendingRead();
    const harness = new PollerHarness(pending.read);
    harness.invalidate();
    pending.resolve(status);
    await vi.advanceTimersByTimeAsync(3000);
    expect(harness.onStatus).not.toHaveBeenCalled();
    harness.invalidate();
    pending.reject(new Error("stale failure"));
    await vi.advanceTimersByTimeAsync(0);
    expect(harness.onError).not.toHaveBeenCalled();
    harness.poller.stop();
  });

  it("skips busy or hidden periods, refreshes immediately on resume, and recovers from errors", async () => {
    vi.useFakeTimers();
    const read = vi.fn(async () => status).mockRejectedValueOnce(new Error("temporary failure"));
    const harness = new PollerHarness(read);
    await vi.advanceTimersByTimeAsync(0);
    expect(harness.onError).toHaveBeenCalledOnce();
    harness.busy = true;
    await vi.advanceTimersByTimeAsync(9000);
    expect(read).toHaveBeenCalledOnce();
    harness.busy = false;
    await harness.poller.refresh();
    expect(harness.onStatus).toHaveBeenCalledWith(status);
    await vi.advanceTimersByTimeAsync(3000);
    expect(read).toHaveBeenCalledTimes(3);
    harness.poller.stop();
  });
});
