import { describe, expect, it } from "vitest";
import { RunningActions, type ActionName } from "../../domain/RunningActions";
import { SteamStatus, type SteamSnapshot } from "../../domain/SteamStatus";
import { TestViewFixtures, type TestViewSteamState } from "../../testview/TestViewFixtures";
import { initialAppState, type AppState } from "../AppState";
import { SteamToolView } from "./SteamToolView";

/** Builds a view of the Steam tool in a given situation. */
class Situation {
  private state: AppState = { ...initialAppState(false, false), status: TestViewFixtures.steamStatus("online") };

  steam(state: TestViewSteamState): this {
    this.state = { ...this.state, status: TestViewFixtures.steamStatus(state) };
    return this;
  }

  running(...busy: ActionName[]): this {
    this.state = { ...this.state, running: RunningActions.of(...busy) };
    return this;
  }

  reading(snapshot: Partial<SteamSnapshot>, memory: Parameters<SteamStatus["with"]>[0] = {}): this {
    const base: SteamSnapshot = { steamPath: "C:\\Steam\\steam.exe", steamRunning: true, steamOnline: false, networkPaused: false };
    this.state = { ...this.state, status: SteamStatus.from({ ...base, ...snapshot }).with(memory) };
    return this;
  }

  with(change: Partial<AppState>): this {
    this.state = { ...this.state, ...change };
    return this;
  }

  get view(): SteamToolView {
    return new SteamToolView(this.state);
  }
}

describe("connection label", () => {
  const label = (situation: Situation) => situation.view.networkLabel;

  it("shows connecting during the grace period and restoring during recovery", () => {
    expect(label(new Situation().reading({ steamOnline: null }, { connectionPending: true }))).toBe("Connecting…");
    expect(label(new Situation().reading({ steamOnline: null }, { restoringSince: 0 }))).toBe("Restoring connection…");
  });

  it("does not claim online just because Steam is running or a pause ended", () => {
    expect(label(new Situation().reading({}))).toBe("Steam offline");
    expect(label(new Situation().reading({ steamOnline: null }))).toBe("Steam connection unavailable");
    expect(label(new Situation().reading({ steamOnline: true }))).toBe("Steam online");
    expect(label(new Situation().reading({ steamOnline: true, networkPaused: true }))).toBe("Steam offline");
    expect(label(new Situation().reading({ steamOnline: true, steamRunning: false }))).toBe("Steam is not running");
  });

  it("identifies retained readings without overriding current paused or stopped states", () => {
    const stale = (snapshot: Partial<SteamSnapshot>) => label(new Situation().reading(snapshot, { connectionStale: true }));
    expect(stale({ steamOnline: true })).toBe("Last confirmed online");
    expect(stale({})).toBe("Last confirmed offline");
    expect(stale({ steamOnline: null })).toBe("Steam connection unavailable");
    expect(stale({ networkPaused: true })).toBe("Steam offline");
    expect(stale({ steamRunning: false })).toBe("Steam is not running");
    expect(stale({ steamPath: null })).toBe("Steam location needed");
  });
});

describe("restart control", () => {
  it("names each restart phase", () => {
    expect(new Situation().with({ restartPhase: "restarting" }).running("restart").view.restartButtonLabel).toBe("Restarting…");
    expect(new Situation().with({ restartPhase: "loading-ui" }).view.restartStatusText).toBe("Loading UI…");
    expect(new Situation().with({ restartPhase: "loaded" }).view.restartStatusText).toBe("Loaded");
    expect(new Situation().steam("stopped").view.restartButtonLabel).toBe("Start Steam");
  });

  it("stays available during a network change, but not during other actions", () => {
    expect(new Situation().running("network").view.canRestart).toBe(true);
    expect(new Situation().running("check-update").view.canRestart).toBe(false);
    expect(new Situation().steam("missing").view.canRestart).toBe(false);
  });
});

describe("network control", () => {
  it("offers to resume a paused network, and says so while resuming", () => {
    const paused = new Situation().steam("paused");
    expect(paused.view).toMatchObject({ networkHeading: "Network paused", networkButtonLabel: "Restore and reconnect", networkIcon: "offline" });
    expect(paused.running("network").view).toMatchObject({
      networkLabel: "Restoring connection…",
      networkButtonLabel: "Restoring and reconnecting…",
      networkIcon: "connecting",
    });
  });

  it("marks an unknown connection instead of guessing", () => {
    expect(new Situation().steam("connecting").view.networkIcon).toBe("unknown");
  });
});

describe("tool summary", () => {
  it("puts errors first, then restarts, then the connection", () => {
    expect(new Situation().with({ error: "Failed" }).view.toolLabel).toBe("Needs attention");
    expect(new Situation().with({ restartPhase: "restarting" }).view).toMatchObject({ toolLabel: "Restarting…", toolIcon: "restarting" });
    expect(new Situation().steam("paused").view.toolLabel).toBe("Steam network paused");
    expect(new Situation().steam("stopped").view.toolLabel).toBe("Steam is not running");
    expect(new Situation().steam("missing").view.toolLabel).toBe("Steam location needed");
  });
});
