import type { Desktop } from "../../platform/Desktop";
import type { ActionHost, ActionScripts, NetworkRequest } from "./ActionScripts";

/** Actions on the real computer, through the desktop backend. */
export class LiveActions implements ActionScripts {
  constructor(private readonly desktop: Desktop, private readonly host: ActionHost) {}

  async restartSteam(force: boolean): Promise<void> {
    const { host } = this;
    host.updateStatus(status => status.with({ steamOnline: null, connectionStale: false }));
    await this.desktop.restartSteam(force, () => host.setRestartPhase("loading-ui"));
    host.updateStatus(status => status.with({ steamRunning: true }));
    host.setRestartPhase("loaded");
    host.showLoadedBriefly();
    const next = await this.desktop.readStatus();
    host.updateStatus(status => status.mergeRestartResult(next));
  }

  async setNetworkPaused({ pause, restartGeneration, resumedAt }: NetworkRequest): Promise<void> {
    const next = await this.desktop.setNetworkPaused(pause);
    const restartedMeanwhile = this.host.restartedSince(restartGeneration);
    this.host.updateStatus(status => status.mergeNetworkResult(next, restartedMeanwhile, resumedAt));
  }

  async chooseSteamExecutable(): Promise<void> {
    const next = await this.desktop.chooseSteamExecutable();
    if (next) this.host.updateStatus(() => next);
  }

  async checkForUpdate(): Promise<void> {
    const available = await this.desktop.findUpdate();
    this.host.setUpdate({ available, error: null, feedback: available ? null : "RBX Tools is up to date." });
  }

  async installUpdate(): Promise<void> {
    await this.desktop.installUpdate();
  }
}
