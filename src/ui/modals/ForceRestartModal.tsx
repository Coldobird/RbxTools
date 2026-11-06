import type { AppController } from "../../app/AppController";
import { ModalHeading } from "../components/ModalFrame";
import { IconRestart } from "../components/PixelIcons";

/** Asked when Steam ignored a normal shutdown request. */
export function ForceRestartModal({ networkPaused, actions }: {
  networkPaused: boolean;
  actions: Pick<AppController, "closeModal" | "restartSteam">;
}) {
  return (
    <>
      <ModalHeading icon={<IconRestart className="accent" size={26} />} title="Force restart Steam?" />
      <p className="modal-intro">
        Force-close Steam and restart it? {networkPaused ? "The network will stay paused. " : ""}This can interrupt downloads and cloud sync.
      </p>
      <div className="modal-actions">
        <button className="secondary-button" onClick={() => actions.closeModal()}><span className="ui-label">Cancel</span></button>
        <button className="primary-button" onClick={() => actions.restartSteam(true)}><span className="ui-label">Force restart</span></button>
      </div>
    </>
  );
}
