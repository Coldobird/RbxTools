import { afterEach, describe, expect, it, vi } from "vitest";
import type { Status } from "./api";
import { sameStatus, startStatusPolling } from "./statusPolling";

const status: Status = {
  steamPath: "C:\\Steam\\steam.exe",
  steamRunning: true,
  targetPath: "C:\\Steam\\steam.exe",
  blocked: false,
  elevated: true,
};

function setup(read: () => Promise<Status> = async () => status) {
  let busy = false;
  let generation = 0;
  const onStatus = vi.fn();
  const onError = vi.fn();
  const poller = startStatusPolling({
    read,
    canPoll: () => !busy,
    generation: () => generation,
    onStatus,
    onError,
  });
  return { ...poller, onStatus, onError, invalidate: () => generation++, setBusy: (value: boolean) => { busy = value; } };
}

afterEach(() => vi.useRealTimers());

describe("status polling", () => {
  it("does not overlap slow requests and stops after cleanup", async () => {
    vi.useFakeTimers();
    let resolve!: (status: Status) => void;
    const read = vi.fn(() => new Promise<Status>((done) => { resolve = done; }));
    const poller = setup(read);
    await vi.advanceTimersByTimeAsync(12000);
    void poller.refresh();
    expect(read).toHaveBeenCalledTimes(1);
    resolve(status);
    await vi.advanceTimersByTimeAsync(3000);
    expect(read).toHaveBeenCalledTimes(2);
    poller.stop();
    resolve(status);
    await vi.advanceTimersByTimeAsync(12000);
    expect(read).toHaveBeenCalledTimes(2);
    expect(poller.onStatus).toHaveBeenCalledTimes(1);
  });

  it("ignores responses and errors from before an action", async () => {
    vi.useFakeTimers();
    let resolve!: (status: Status) => void;
    let reject!: (error: Error) => void;
    const read = () => new Promise<Status>((done, fail) => { resolve = done; reject = fail; });
    const poller = setup(read);
    poller.invalidate();
    resolve(status);
    await vi.advanceTimersByTimeAsync(3000);
    expect(poller.onStatus).not.toHaveBeenCalled();
    poller.invalidate();
    reject(new Error("stale failure"));
    await vi.advanceTimersByTimeAsync(0);
    expect(poller.onError).not.toHaveBeenCalled();
    poller.stop();
  });

  it("skips busy or hidden periods, refreshes immediately on resume, and recovers from errors", async () => {
    vi.useFakeTimers();
    const read = vi.fn(async () => status).mockRejectedValueOnce(new Error("temporary failure"));
    const poller = setup(read);
    await vi.advanceTimersByTimeAsync(0);
    expect(poller.onError).toHaveBeenCalledOnce();
    poller.setBusy(true);
    await vi.advanceTimersByTimeAsync(9000);
    expect(read).toHaveBeenCalledOnce();
    poller.setBusy(false);
    await poller.refresh();
    expect(poller.onStatus).toHaveBeenCalledWith(status);
    await vi.advanceTimersByTimeAsync(3000);
    expect(read).toHaveBeenCalledTimes(3);
    poller.stop();
  });

  it("identifies every visible state change", () => {
    expect(sameStatus(status, { ...status })).toBe(true);
    for (const change of [
      { steamPath: null }, { targetPath: null }, { steamRunning: false },
      { blocked: true }, { elevated: false },
    ]) expect(sameStatus(status, { ...status, ...change })).toBe(false);
  });
});
