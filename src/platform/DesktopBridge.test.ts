import { afterEach, describe, expect, it, vi } from "vitest";
import { TestViewMode } from "../testview/TestViewMode";
import { DesktopBridge } from "./DesktopBridge";
import { DesktopError } from "./DesktopError";
import { Timings } from "./Timings";

const tauri = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: tauri.invoke, Channel: class { onmessage = () => {}; } }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: tauri.open }));

const snapshot = { steamPath: "C:\\Steam\\steam.exe", steamRunning: true, steamOnline: true, networkPaused: false };

/** A bridge in one of the three environments the app runs in. */
class Bridges {
  static desktop(): DesktopBridge {
    return new DesktopBridge(true, new TestViewMode(false), new Timings());
  }

  static browserPreview(): DesktopBridge {
    return new DesktopBridge(false, new TestViewMode(false), new Timings());
  }

  static uiTestView(): { bridge: DesktopBridge; mode: TestViewMode } {
    const mode = new TestViewMode(true);
    return { bridge: new DesktopBridge(true, mode, new Timings()), mode };
  }
}

/** Every call that would change the computer. */
const systemActions = (bridge: DesktopBridge) => [
  () => bridge.restartSteam(false), () => bridge.restartSteam(true),
  () => bridge.setNetworkPaused(true), () => bridge.setNetworkPaused(false),
  () => bridge.installUpdate(), () => bridge.chooseSteamExecutable(),
];

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("browser preview", () => {
  it("does not present fake Steam or network state", async () => {
    expect(await Bridges.browserPreview().readStatus()).toMatchObject({
      steamPath: null, steamRunning: false, steamOnline: false, networkPaused: false,
    });
  });

  it("cannot restart, pause, install, or find updates outside the desktop app", async () => {
    const bridge = Bridges.browserPreview();
    for (const action of systemActions(bridge)) await expect(action()).rejects.toThrow("desktop app");
    expect(await bridge.findUpdate()).toBeNull();
    expect(tauri.invoke).not.toHaveBeenCalled();
  });
});

describe("UI test view", () => {
  it("blocks every system action and file dialog even inside the desktop app", async () => {
    const { bridge } = Bridges.uiTestView();
    for (const action of systemActions(bridge)) await expect(action()).rejects.toThrow("disabled in UI test view");
    await bridge.readStatus();
    expect(await bridge.findUpdate()).toBeNull();
    expect(tauri.invoke).not.toHaveBeenCalled();
    expect(tauri.open).not.toHaveBeenCalled();
  });

  it("restores normal access when the test view is exited", async () => {
    const { bridge, mode } = Bridges.uiTestView();
    await bridge.readStatus();
    mode.set(false);
    tauri.invoke.mockResolvedValueOnce(snapshot);
    expect(await bridge.readStatus()).toMatchObject({ steamOnline: true });
    expect(tauri.invoke).toHaveBeenCalledExactlyOnceWith("get_status", undefined);
  });
});

describe("desktop errors", () => {
  it("keeps the backend's error kind and message", async () => {
    tauri.invoke.mockRejectedValueOnce({ kind: "steam-still-running", message: "Steam is still running." });
    const error = await Bridges.desktop().restartSteam(false).catch(DesktopError.from);
    expect(error).toMatchObject({ kind: "steam-still-running", message: "Steam is still running." });
  });

  it("retries a status check while the previous one is still running", async () => {
    vi.useFakeTimers();
    tauri.invoke
      .mockRejectedValueOnce({ kind: "busy", message: "Steam status check is still pending." })
      .mockResolvedValueOnce(snapshot);
    const reading = Bridges.desktop().readStatus();
    await vi.advanceTimersByTimeAsync(500);
    expect(await reading).toMatchObject({ steamOnline: true });
    expect(tauri.invoke).toHaveBeenCalledTimes(2);
  });
});

describe("desktop status request", () => {
  it("rejects a hung status request after eight seconds", async () => {
    vi.useFakeTimers();
    tauri.invoke.mockReturnValue(new Promise(() => {}));
    const rejection = expect(Bridges.desktop().readStatus()).rejects.toThrow("Steam status check timed out after 8 seconds.");
    await vi.advanceTimersByTimeAsync(8_000);
    await rejection;
    expect(vi.getTimerCount()).toBe(0);
  });

  it("ignores a late result after timeout and accepts the next status request", async () => {
    vi.useFakeTimers();
    const bridge = Bridges.desktop();
    let resolveFirst!: (value: object) => void;
    tauri.invoke
      .mockReturnValueOnce(new Promise(resolve => { resolveFirst = resolve; }))
      .mockResolvedValueOnce(snapshot);

    const rejection = expect(bridge.readStatus()).rejects.toThrow("Steam status check timed out after 8 seconds.");
    await vi.advanceTimersByTimeAsync(8_000);
    await rejection;

    resolveFirst({ ...snapshot, steamOnline: false });
    await Promise.resolve();
    expect(await bridge.readStatus()).toMatchObject({ steamOnline: true });
    expect(tauri.invoke).toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(0);
  });
});
