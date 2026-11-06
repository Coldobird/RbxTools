import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SteamStatus, type SteamSnapshot } from "../domain/SteamStatus";
import type { AvailableUpdate, Desktop } from "../platform/Desktop";
import { DesktopError } from "../platform/DesktopError";
import type { PageVisibility } from "../platform/PageVisibility";
import { Timings } from "../platform/Timings";
import { TestViewMode } from "../testview/TestViewMode";
import { AppController } from "./AppController";
import { SteamToolView } from "./views/SteamToolView";

/** A stand-in desktop backend with a scriptable Steam. */
class FakeDesktop implements Desktop {
  readonly isDesktop = true;
  readonly appVersion = "1.0.0";
  steam: SteamSnapshot = { steamPath: "C:\\Steam\\steam.exe", steamRunning: true, steamOnline: true, networkPaused: false };
  update: AvailableUpdate | null = null;
  statusError: DesktopError | null = null;
  /** Steam ignores graceful shutdowns. */
  stuck = false;
  readonly calls: string[] = [];

  constructor(private readonly testMode: TestViewMode) {}

  get isLive(): boolean {
    return !this.testMode.isActive;
  }

  async readStatus(): Promise<SteamStatus> {
    if (this.statusError) throw this.statusError;
    return SteamStatus.from(this.steam);
  }

  async chooseSteamExecutable(): Promise<SteamStatus | null> {
    this.calls.push("choose");
    return null;
  }

  async restartSteam(force: boolean, onLoadingUi?: () => void): Promise<void> {
    this.calls.push(force ? "force-restart" : "restart");
    if (this.stuck && !force) throw new DesktopError("steam-still-running", "Steam is still running.");
    onLoadingUi?.();
  }

  async setNetworkPaused(paused: boolean): Promise<SteamStatus> {
    this.calls.push(paused ? "pause" : "resume");
    this.steam = { ...this.steam, networkPaused: paused, steamOnline: paused ? false : this.steam.steamOnline };
    return SteamStatus.from(this.steam);
  }

  async findUpdate(): Promise<AvailableUpdate | null> {
    return this.update;
  }

  async installUpdate(): Promise<void> {
    this.calls.push("install");
  }
}

/** A window that is always visible. */
class VisibleWindow implements PageVisibility {
  readonly isHidden = false;
  onVisible(): () => void {
    return () => {};
  }
}

/** A controller over a fake desktop, live or in the UI test view. */
class Harness {
  readonly testMode: TestViewMode;
  readonly desktop: FakeDesktop;
  readonly controller: AppController;

  constructor(testView = false) {
    this.testMode = new TestViewMode(testView);
    this.desktop = new FakeDesktop(this.testMode);
    this.controller = new AppController({
      desktop: this.desktop,
      testView: this.testMode,
      timings: new Timings(),
      visibility: new VisibleWindow(),
    });
  }

  get state() {
    return this.controller.getSnapshot();
  }

  get steamTool(): SteamToolView {
    return new SteamToolView(this.state);
  }

  /** Lets pending promises and timers up to `ms` run. */
  async wait(ms = 0): Promise<void> {
    await vi.advanceTimersByTimeAsync(ms);
  }
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("live desktop", () => {
  it("shows the first status reading", async () => {
    const app = new Harness();
    expect(app.state.loading).toBe(true);
    app.controller.start();
    await app.wait();
    expect(app.state.loading).toBe(false);
    expect(app.steamTool.networkLabel).toBe("Steam online");
    app.controller.stop();
  });

  it("asks before forcing a restart that Steam ignored, then forces it", async () => {
    const app = new Harness();
    app.desktop.stuck = true;
    app.controller.start();
    await app.wait();
    app.controller.restartSteam();
    await app.wait();
    expect(app.state).toMatchObject({ modal: "force", restartPhase: null, error: null });
    app.controller.restartSteam(true);
    await app.wait();
    expect(app.desktop.calls).toEqual(["restart", "force-restart"]);
    expect(app.state).toMatchObject({ modal: null, restartPhase: "loaded" });
    expect(app.state.running.isIdle).toBe(true);
    app.controller.stop();
  });

  it("keeps showing the restoring label until Steam confirms it is online", async () => {
    const app = new Harness();
    app.desktop.steam = { ...app.desktop.steam, networkPaused: true, steamOnline: false };
    app.controller.start();
    await app.wait();
    app.desktop.steam = { ...app.desktop.steam, steamOnline: false };
    app.controller.toggleNetwork();
    await app.wait();
    expect(app.desktop.calls).toEqual(["resume"]);
    expect(app.steamTool.networkLabel).toBe("Restoring connection…");
    app.desktop.steam = { ...app.desktop.steam, steamOnline: true };
    await app.wait(3000);
    expect(app.steamTool.networkLabel).toBe("Steam online");
    app.controller.stop();
  });

  it("reports failed status checks only after the 30-second grace period", async () => {
    const app = new Harness();
    app.controller.start();
    await app.wait();
    app.desktop.statusError = new DesktopError("failed", "Steam stopped answering.");
    await app.wait(27_000);
    expect(app.state.error).toBeNull();
    expect(app.steamTool.networkLabel).toBe("Steam online");
    await app.wait(6_000);
    expect(app.state.error).toBe("Steam stopped answering.");
    expect(app.steamTool.networkLabel).toBe("Last confirmed online");
    app.controller.stop();
  });

  it("offers a startup update in a dialog once nothing else is running", async () => {
    const app = new Harness();
    app.desktop.update = { version: "2.0.0" };
    app.controller.start();
    await app.wait();
    expect(app.state).toMatchObject({ modal: "update", update: { available: { version: "2.0.0" } } });
    app.controller.stop();
  });
});

describe("UI test view", () => {
  it("simulates actions without touching the desktop", async () => {
    const app = new Harness(true);
    app.controller.start();
    app.controller.testViewControls.simulateSteam("stopped");
    app.controller.restartSteam();
    expect(app.steamTool.restartStatusText).toBe("Restarting…");
    await app.wait(650);
    expect(app.steamTool.restartStatusText).toBe("Loading UI…");
    await app.wait(650);
    expect(app.steamTool.restartStatusText).toBe("Loaded");
    expect(app.state.status.steamRunning).toBe(true);
    expect(app.desktop.calls).toEqual([]);
  });

  it("simulated busy states block the actions they stand for", async () => {
    const app = new Harness(true);
    app.controller.testViewControls.simulateBusy("check-update");
    expect(app.state.running.has("check-update")).toBe(true);
    expect(app.steamTool.canRestart).toBe(false);
    app.controller.restartSteam();
    expect(app.state.restartPhase).toBeNull();
    app.controller.testViewControls.simulateBusy("none");
    expect(app.steamTool.canRestart).toBe(true);
  });

  it("mirrors the simulated Steam state in the panel once actions finish", async () => {
    const app = new Harness(true);
    app.controller.toggleNetwork();
    await app.wait(650);
    expect(app.controller.testViewControls.steamState).toBe("paused");
    expect(app.steamTool.networkHeading).toBe("Network paused");
  });
});
