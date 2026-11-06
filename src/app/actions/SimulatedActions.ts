import { TestViewFixtures } from "../../testview/TestViewFixtures";
import type { ActionHost, ActionScripts, NetworkRequest } from "./ActionScripts";

const STEP_MS = 650;

/**
 * Plays each action out on screen without touching the computer, for the
 * browser preview and the UI test view.
 */
export class SimulatedActions implements ActionScripts {
  constructor(private readonly host: ActionHost) {}

  async restartSteam(): Promise<void> {
    const { host } = this;
    await this.step();
    host.setRestartPhase("loading-ui");
    await this.step();
    host.updateStatus(status => status.with({ steamRunning: true, steamOnline: !status.networkPaused, connectionStale: false }));
    host.setRestartPhase("loaded");
    // The test view keeps "Loaded" on screen for inspection.
    if (!host.isTestView()) host.showLoadedBriefly();
  }

  async setNetworkPaused({ pause }: NetworkRequest): Promise<void> {
    await this.step();
    this.host.updateStatus(status => status.with({
      networkPaused: pause,
      steamOnline: !pause && status.steamRunning,
      connectionStale: false,
    }));
  }

  async chooseSteamExecutable(): Promise<void> {
    await this.step();
    this.host.updateStatus(status => status.with({ steamPath: TestViewFixtures.steamPath }));
  }

  async checkForUpdate(): Promise<void> {
    await this.step();
    this.host.setUpdate(this.host.isTestView()
      ? { available: TestViewFixtures.update, error: null, feedback: null }
      : { feedback: "Update check complete (preview only). No update was installed." });
  }

  async installUpdate(): Promise<void> {
    await this.step();
    this.host.setUpdate({ feedback: "Preview update complete. No files were changed." });
    this.host.closeModal("update");
  }

  private step(): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, STEP_MS));
  }
}
