import { describe, expect, it } from "vitest";
import { SteamStatus, type SteamSnapshot } from "./SteamStatus";

/** Builds readings of one Steam installation that differ only where a test says so. */
class Readings {
  static readonly base: SteamSnapshot = { steamPath: "C:\\Steam\\steam.exe", steamRunning: true, steamOnline: true, networkPaused: false };

  static online(changes: Partial<SteamSnapshot> = {}): SteamStatus {
    return SteamStatus.from({ ...Readings.base, ...changes });
  }

  static missing(changes: Partial<SteamSnapshot> = {}): SteamStatus {
    return Readings.online({ steamOnline: null, ...changes });
  }
}

describe("equality", () => {
  it("identifies every visible state change", () => {
    const status = Readings.online();
    expect(status.equals(Readings.online())).toBe(true);
    for (const changed of [
      Readings.online({ steamPath: null }), Readings.online({ steamRunning: false }),
      Readings.online({ steamOnline: false }), Readings.missing(),
      status.with({ connectionStale: true }), Readings.online({ networkPaused: true }),
    ]) expect(status.equals(changed)).toBe(false);
  });
});

describe("last confirmed connection", () => {
  it("keeps restoring through missing or offline readings until confirmation or 30 seconds", () => {
    const restoring = Readings.missing().with({ restoringSince: 0 });
    expect(restoring.mergePolled(Readings.missing(), false, 29_999).restoringSince).toBe(0);
    expect(restoring.mergePolled(Readings.online({ steamOnline: false }), false, 29_999).restoringSince).toBe(0);
    expect(restoring.mergePolled(Readings.missing(), true, 30_000).restoringSince).toBeUndefined();
    expect(restoring.mergePolled(Readings.online(), false, 1000).restoringSince).toBeUndefined();
  });

  it.each([true, false])("retains confirmed %s across unavailable checks, then accepts a real response", online => {
    const confirmed = Readings.online({ steamOnline: online });
    const retained = confirmed.mergePolled(Readings.missing());
    expect(retained).toMatchObject({ steamOnline: online, connectionStale: true });
    expect(retained.mergePolled(Readings.missing())).toBe(retained);
    expect(retained.mergePolled(Readings.online({ steamOnline: !online })))
      .toMatchObject({ steamOnline: !online, connectionStale: false });
  });

  it("does not invent a connection before the first successful check", () => {
    expect(Readings.missing().mergePolled(Readings.missing())).toMatchObject({ steamOnline: null });
  });

  it.each([
    { steamPath: "C:\\Other\\steam.exe" },
    { steamPath: null },
    { steamRunning: false },
    { networkPaused: true },
  ])("invalidates the assumption when the Steam context changes: %j", change => {
    expect(Readings.online().mergePolled(Readings.missing(change)))
      .toMatchObject({ steamOnline: null, connectionStale: false });
  });

  it("does not reuse connection from a stopped, networkPaused, or restarted session", () => {
    for (const previous of [Readings.online({ steamRunning: false }), Readings.online({ networkPaused: true }), Readings.missing()]) {
      expect(previous.mergePolled(Readings.missing())).toMatchObject({ steamOnline: null });
    }
  });
});

describe("concurrent action results", () => {
  it("restart cannot undo a newer network pause or restore", () => {
    expect(Readings.online({ networkPaused: true }).mergeRestartResult(Readings.online()))
      .toMatchObject({ steamRunning: true, networkPaused: true, steamOnline: false });
    expect(Readings.online().mergeRestartResult(Readings.online({ networkPaused: true, steamOnline: false })))
      .toMatchObject({ steamRunning: true, networkPaused: false, steamOnline: null });
  });

  it("network results cannot undo startup or apply a connection response from an older session", () => {
    const status = Readings.online();
    expect(status.mergeNetworkResult(Readings.online({ steamRunning: false, steamOnline: false }), true))
      .toMatchObject({ steamRunning: true, networkPaused: false, steamOnline: null });
    expect(status.mergeNetworkResult(Readings.online({ networkPaused: true }), true))
      .toMatchObject({ steamRunning: true, networkPaused: true, steamOnline: false });
    expect(status.mergeNetworkResult(Readings.online({ steamOnline: false }), false))
      .toMatchObject({ steamRunning: true, steamOnline: false });
  });
});
