import type { AppState } from "../AppState";
import type { AppAbout } from "./AppAbout";
import { StateView } from "./StateView";
import { UpdateView } from "./UpdateView";

/** What the Settings dialog shows. */
export class SettingsView extends StateView {
  readonly update: UpdateView;

  constructor(state: AppState, private readonly about: AppAbout) {
    super(state);
    this.update = new UpdateView(state);
  }

  get steamPathLabel(): string {
    return this.state.status.steamPath ?? "Not detected yet";
  }

  /** The version, plus the edition outside the test view. */
  get footnote(): string {
    const version = `RBX Tools v${this.about.appVersion}`;
    if (this.state.testView) return version;
    return `${version} · ${this.about.isDesktop ? "Windows desktop edition" : "Browser preview · no system changes"}`;
  }
}
