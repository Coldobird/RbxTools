import { afterEach, describe, expect, it, vi } from "vitest";
import { isUiTestRequested, setUiTestActive, testSteamStatus } from "./uiTestView";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
import { getStatus, getUpdate, installUpdate, makeSpacewarPrivate, pickExecutable, restartSteam, setBlocked } from "./api";

afterEach(() => { setUiTestActive(false); vi.clearAllMocks(); });

describe("UI test view", () => {
  it("requires an explicit activation parameter", () => {
    expect(isUiTestRequested("?ui-test=1")).toBe(true);
    expect(isUiTestRequested("?other=1&ui-test=1")).toBe(true);
    expect(isUiTestRequested("?ui-test=0")).toBe(false);
    expect(isUiTestRequested("?other=ui-test=1")).toBe(false);
    expect(isUiTestRequested("")).toBe(false);
  });

  it("blocks every system mutation and file dialog even inside Tauri", async () => {
    setUiTestActive(true);
    for (const operation of [
      () => restartSteam(false), () => restartSteam(true), () => setBlocked(true),
      () => setBlocked(false), () => makeSpacewarPrivate(), () => installUpdate(),
      () => pickExecutable("steam"), () => pickExecutable("target"),
    ]) await expect(operation()).rejects.toThrow("disabled in UI test view");
    await getStatus();
    expect(await getUpdate()).toEqual({ update: null });
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it("restores normal API access when test view is exited", async () => {
    setUiTestActive(true);
    await getStatus();
    setUiTestActive(false);
    mocks.invoke.mockResolvedValueOnce(testSteamStatus("online"));
    expect(await getStatus()).toEqual(testSteamStatus("online"));
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_status");
  });

  it("represents disconnected and unknown status without claiming online", () => {
    expect(testSteamStatus("online").steamOnline).toBe(true);
    expect(testSteamStatus("offline").steamOnline).toBe(false);
    expect(testSteamStatus("connecting").steamOnline).toBeNull();
    expect(testSteamStatus("blocked").blocked).toBe(true);
    expect(testSteamStatus("blocked").steamOnline).toBe(false);
    expect(testSteamStatus("stopped").steamRunning).toBe(false);
    expect(testSteamStatus("missing").steamPath).toBeNull();
  });
});
