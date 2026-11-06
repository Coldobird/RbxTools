import type { AppController } from "../../app/AppController";
import type { SettingsView } from "../../app/views/SettingsView";
import { ModalHeading } from "../components/ModalFrame";
import { IconSettings } from "../components/PixelIcons";

type SettingsActions = Pick<AppController, "chooseSteamExecutable" | "updateFromSettings">;

/** Steam location, updates, and the app version. */
export function SettingsModal({ view, actions }: { view: SettingsView; actions: SettingsActions }) {
  return (
    <>
      <ModalHeading icon={<IconSettings className="accent" size={26} />} title="Settings" />
      <div className="setting-row">
        <div>
          <strong>Steam location</strong>
          <p className="path-label">{view.steamPathLabel}</p>
        </div>
        <button className="small-button" disabled={view.isBusy} onClick={() => actions.chooseSteamExecutable()}>
          <span className="ui-label">Browse</span>
        </button>
      </div>
      <div className="setting-row">
        <div>
          <strong>{view.update.settingsTitle}</strong>
          <p>{view.update.settingsDescription}</p>
        </div>
        <button className="small-button" disabled={view.isBusy} onClick={() => actions.updateFromSettings()}>
          <span className="ui-label">{view.update.settingsButtonLabel}</span>
        </button>
      </div>
      <p className="modal-footnote">{view.footnote}</p>
    </>
  );
}
