import { StateView } from "./StateView";

/** What the Settings row, the update dialog, and the update toast show. */
export class UpdateView extends StateView {
  get isInstalling(): boolean {
    return this.isRunning("install-update");
  }

  get isChecking(): boolean {
    return this.isRunning("check-update");
  }

  get version(): string | null {
    return this.state.update.available?.version ?? null;
  }

  get error(): string | null {
    return this.state.update.error;
  }

  /** An update can be installed now: one is available and nothing else runs. */
  get canInstall(): boolean {
    return this.version !== null && !this.isBusy;
  }

  get settingsTitle(): string {
    return this.version ? `Update v${this.version} ready` : "Updates";
  }

  get settingsDescription(): string {
    const { error, feedback } = this.state.update;
    return error ?? feedback ?? (this.version
      ? "Download from GitHub and restart to finish updating."
      : "Checks GitHub automatically when you open the app.");
  }

  get settingsButtonLabel(): string {
    if (this.isInstalling) return "Downloading…";
    if (this.isChecking) return "Checking…";
    return this.version ? "Update" : "Check for updates";
  }

  get installButtonLabel(): string {
    return this.isInstalling ? "Downloading…" : "Update";
  }
}
