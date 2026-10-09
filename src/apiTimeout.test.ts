import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

import { getStatus } from "./api";

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("desktop status request", () => {
  it("rejects a hung status request after eight seconds", async () => {
    vi.useFakeTimers();
    mocks.invoke.mockReturnValue(new Promise(() => {}));

    const statusRequest = getStatus();
    const rejection = expect(statusRequest).rejects.toThrow(
      "Steam status check timed out after 8 seconds.",
    );
    await vi.advanceTimersByTimeAsync(8_000);
    await rejection;
    expect(vi.getTimerCount()).toBe(0);
  });

  it("ignores a late result after timeout and accepts the next status request", async () => {
    vi.useFakeTimers();
    let resolveFirst!: (value: { steamOnline: boolean }) => void;
    mocks.invoke
      .mockReturnValueOnce(
        new Promise<{ steamOnline: boolean }>((resolve) => {
          resolveFirst = resolve;
        }),
      )
      .mockResolvedValueOnce({ steamOnline: true });

    const statusRequest = getStatus();
    const rejection = expect(statusRequest).rejects.toThrow(
      "Steam status check timed out after 8 seconds.",
    );
    await vi.advanceTimersByTimeAsync(8_000);
    await rejection;

    resolveFirst({ steamOnline: false });
    await Promise.resolve();
    expect(await getStatus()).toEqual({ steamOnline: true });
    expect(mocks.invoke).toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(0);
  });
});
